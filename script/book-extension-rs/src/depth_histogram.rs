use std::collections::BTreeMap;

use rusqlite::{Connection, Transaction, TransactionBehavior, params};

pub const DEPTH_BUCKET_WIDTH: usize = 10;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepthHistogramBin {
    pub bucket_start: usize,
    pub count: u64,
    pub depth_sum: u64,
    pub max_depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DepthHistogramSeries {
    pub lane: String,
    pub event_kind: String,
    pub bins: Vec<DepthHistogramBin>,
}

impl DepthHistogramSeries {
    pub fn count(&self) -> u64 {
        self.bins.iter().map(|bin| bin.count).sum()
    }

    pub fn average_depth(&self) -> f64 {
        let count = self.count();
        if count == 0 {
            return 0.0;
        }
        self.bins.iter().map(|bin| bin.depth_sum).sum::<u64>() as f64 / count as f64
    }

    pub fn max_depth(&self) -> usize {
        self.bins.iter().map(|bin| bin.max_depth).max().unwrap_or(0)
    }

    pub fn percentile_bucket(&self, percentile: u64) -> Option<usize> {
        let count = self.count();
        if count == 0 {
            return None;
        }
        let target = (count * percentile).div_ceil(100).max(1);
        let mut seen = 0_u64;
        self.bins.iter().find_map(|bin| {
            seen += bin.count;
            (seen >= target).then_some(bin.bucket_start)
        })
    }

    pub fn format_bins(&self) -> String {
        self.bins
            .iter()
            .map(|bin| {
                format!(
                    "{}-{}:{}",
                    bin.bucket_start,
                    bin.bucket_start + DEPTH_BUCKET_WIDTH - 1,
                    bin.count
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DepthHistogramReport {
    pub report_id: i64,
    pub started_at_unix: f64,
    pub finished_at_unix: f64,
    pub from_book_revision: i64,
    pub to_book_revision: i64,
    pub series: Vec<DepthHistogramSeries>,
}

#[derive(Clone, Debug)]
struct CounterRow {
    lane: String,
    event_kind: String,
    bucket_start: usize,
    count: u64,
    depth_sum: u64,
    max_depth: usize,
}

pub fn create_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS extension_depth_state (
             singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
             last_report_at_unix REAL NOT NULL,
             last_report_revision INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS extension_depth_counter (
             lane TEXT NOT NULL,
             event_kind TEXT NOT NULL,
             bucket_start INTEGER NOT NULL CHECK(bucket_start >= 0),
             pending_count INTEGER NOT NULL CHECK(pending_count >= 0),
             pending_depth_sum INTEGER NOT NULL CHECK(pending_depth_sum >= 0),
             pending_max_depth INTEGER,
             total_count INTEGER NOT NULL CHECK(total_count >= 0),
             total_depth_sum INTEGER NOT NULL CHECK(total_depth_sum >= 0),
             total_max_depth INTEGER NOT NULL CHECK(total_max_depth >= 0),
             PRIMARY KEY(lane, event_kind, bucket_start)
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS extension_depth_report (
             report_id INTEGER PRIMARY KEY,
             reported_by_run_id TEXT NOT NULL,
             started_at_unix REAL NOT NULL,
             finished_at_unix REAL NOT NULL,
             from_book_revision INTEGER NOT NULL,
             to_book_revision INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS extension_depth_report_bin (
             report_id INTEGER NOT NULL REFERENCES extension_depth_report(report_id) ON DELETE CASCADE,
             lane TEXT NOT NULL,
             event_kind TEXT NOT NULL,
             bucket_start INTEGER NOT NULL,
             count INTEGER NOT NULL CHECK(count > 0),
             depth_sum INTEGER NOT NULL CHECK(depth_sum >= 0),
             max_depth INTEGER NOT NULL CHECK(max_depth >= 0),
             PRIMARY KEY(report_id, lane, event_kind, bucket_start)
         ) WITHOUT ROWID;",
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO extension_depth_state(
             singleton, last_report_at_unix, last_report_revision
         )
         SELECT 1, CAST(strftime('%s','now') AS REAL), book_revision
         FROM book_meta WHERE singleton = 1",
        [],
    )?;
    Ok(())
}

pub fn record_in_transaction(
    transaction: &Transaction<'_>,
    lane: &str,
    event_kind: &str,
    depth: usize,
) -> Result<(), rusqlite::Error> {
    let bucket_start = depth / DEPTH_BUCKET_WIDTH * DEPTH_BUCKET_WIDTH;
    transaction.execute(
        "INSERT INTO extension_depth_counter(
             lane, event_kind, bucket_start,
             pending_count, pending_depth_sum, pending_max_depth,
             total_count, total_depth_sum, total_max_depth
         ) VALUES(?1, ?2, ?3, 1, ?4, ?4, 1, ?4, ?4)
         ON CONFLICT(lane, event_kind, bucket_start) DO UPDATE SET
             pending_count = pending_count + 1,
             pending_depth_sum = pending_depth_sum + excluded.pending_depth_sum,
             pending_max_depth = MAX(COALESCE(pending_max_depth, 0), excluded.pending_max_depth),
             total_count = total_count + 1,
             total_depth_sum = total_depth_sum + excluded.total_depth_sum,
             total_max_depth = MAX(total_max_depth, excluded.total_max_depth)",
        params![lane, event_kind, bucket_start as i64, depth as i64],
    )?;
    Ok(())
}

pub fn record(
    connection: &mut Connection,
    lane: &str,
    event_kind: &str,
    depth: usize,
) -> Result<(), rusqlite::Error> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    record_in_transaction(&transaction, lane, event_kind, depth)?;
    transaction.commit()
}

pub fn flush(
    connection: &mut Connection,
    run_id: &str,
    now: f64,
) -> Result<Option<DepthHistogramReport>, rusqlite::Error> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let rows = load_rows(
        &transaction,
        "SELECT lane,event_kind,bucket_start,pending_count,pending_depth_sum,pending_max_depth
         FROM extension_depth_counter WHERE pending_count > 0
         ORDER BY lane,event_kind,bucket_start",
    )?;
    if rows.is_empty() {
        return Ok(None);
    }
    let (started_at_unix, from_book_revision): (f64, i64) = transaction.query_row(
        "SELECT last_report_at_unix,last_report_revision
         FROM extension_depth_state WHERE singleton = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let to_book_revision: i64 = transaction.query_row(
        "SELECT book_revision FROM book_meta WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    transaction.execute(
        "INSERT INTO extension_depth_report(
             reported_by_run_id,started_at_unix,finished_at_unix,
             from_book_revision,to_book_revision
         ) VALUES(?1,?2,?3,?4,?5)",
        params![
            run_id,
            started_at_unix,
            now,
            from_book_revision,
            to_book_revision
        ],
    )?;
    let report_id = transaction.last_insert_rowid();
    {
        let mut insert = transaction.prepare(
            "INSERT INTO extension_depth_report_bin(
                 report_id,lane,event_kind,bucket_start,count,depth_sum,max_depth
             ) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        )?;
        for row in &rows {
            insert.execute(params![
                report_id,
                row.lane,
                row.event_kind,
                row.bucket_start as i64,
                row.count as i64,
                row.depth_sum as i64,
                row.max_depth as i64,
            ])?;
        }
    }
    transaction.execute(
        "UPDATE extension_depth_counter
         SET pending_count=0,pending_depth_sum=0,pending_max_depth=NULL
         WHERE pending_count > 0",
        [],
    )?;
    transaction.execute(
        "UPDATE extension_depth_state
         SET last_report_at_unix=?1,last_report_revision=?2 WHERE singleton=1",
        params![now, to_book_revision],
    )?;
    transaction.commit()?;
    Ok(Some(DepthHistogramReport {
        report_id,
        started_at_unix,
        finished_at_unix: now,
        from_book_revision,
        to_book_revision,
        series: group_rows(rows),
    }))
}

pub fn totals(connection: &Connection) -> Result<Vec<DepthHistogramSeries>, rusqlite::Error> {
    let rows = load_rows(
        connection,
        "SELECT lane,event_kind,bucket_start,total_count,total_depth_sum,total_max_depth
         FROM extension_depth_counter WHERE total_count > 0
         ORDER BY lane,event_kind,bucket_start",
    )?;
    Ok(group_rows(rows))
}

fn load_rows(connection: &Connection, sql: &str) -> Result<Vec<CounterRow>, rusqlite::Error> {
    let mut statement = connection.prepare(sql)?;
    statement
        .query_map([], |row| {
            Ok(CounterRow {
                lane: row.get(0)?,
                event_kind: row.get(1)?,
                bucket_start: row.get::<_, i64>(2)? as usize,
                count: row.get::<_, i64>(3)? as u64,
                depth_sum: row.get::<_, i64>(4)? as u64,
                max_depth: row.get::<_, i64>(5)? as usize,
            })
        })?
        .collect()
}

fn group_rows(rows: Vec<CounterRow>) -> Vec<DepthHistogramSeries> {
    let mut grouped: BTreeMap<(String, String), Vec<DepthHistogramBin>> = BTreeMap::new();
    for row in rows {
        grouped
            .entry((row.lane, row.event_kind))
            .or_default()
            .push(DepthHistogramBin {
                bucket_start: row.bucket_start,
                count: row.count,
                depth_sum: row.depth_sum,
                max_depth: row.max_depth,
            });
    }
    grouped
        .into_iter()
        .map(|((lane, event_kind), bins)| DepthHistogramSeries {
            lane,
            event_kind,
            bins,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_formats_bins_and_bucket_percentiles() {
        let series = DepthHistogramSeries {
            lane: "normal".to_owned(),
            event_kind: "search".to_owned(),
            bins: vec![
                DepthHistogramBin {
                    bucket_start: 10,
                    count: 1,
                    depth_sum: 15,
                    max_depth: 15,
                },
                DepthHistogramBin {
                    bucket_start: 30,
                    count: 3,
                    depth_sum: 102,
                    max_depth: 39,
                },
            ],
        };
        assert_eq!(series.count(), 4);
        assert_eq!(series.average_depth(), 29.25);
        assert_eq!(series.percentile_bucket(50), Some(30));
        assert_eq!(series.percentile_bucket(90), Some(30));
        assert_eq!(series.max_depth(), 39);
        assert_eq!(series.format_bins(), "10-19:1,30-39:3");
    }
}
