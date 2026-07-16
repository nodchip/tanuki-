use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::Instant,
};

use rusqlite::{Connection, params};
use serde::Serialize;
use thiserror::Error;

use crate::{
    book::{BookEntry, BookParseError, parse_book_entry_line, strip_sfen_ply},
    validation::legal_distinct_entry_count,
};

#[derive(Clone, Debug, Serialize)]
pub struct CoverageReport {
    pub snapshot_id: String,
    pub book_hash: String,
    pub corpus_revision: i64,
    pub overall: GroupCoverage,
    pub by_site: BTreeMap<String, GroupCoverage>,
    pub by_event: BTreeMap<String, GroupCoverage>,
    pub by_bucket: BTreeMap<String, GroupCoverage>,
    pub by_ply_band: BTreeMap<String, GroupCoverage>,
    pub by_side: BTreeMap<String, GroupCoverage>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GroupCoverage {
    pub unique: CoverageMetric,
    pub occurrences: CoverageMetric,
}

#[derive(Clone, Debug, Serialize)]
pub struct CoverageMetric {
    pub covered: i64,
    pub total: i64,
    pub rate: f64,
}

pub fn generate_coverage_report(
    connection: &mut Connection,
    book_path: &Path,
    snapshot_id: &str,
    book_hash: &str,
) -> Result<CoverageReport, CoverageError> {
    connection.execute_batch(
        "PRAGMA temp_store=FILE;
         DROP TABLE IF EXISTS temp.covered_book;
         DROP TABLE IF EXISTS temp.covered_position;
         CREATE TEMP TABLE covered_book(position_key TEXT PRIMARY KEY) WITHOUT ROWID;",
    )?;
    eprintln!(
        "[corpus] phase=coverage event=book_scan_start path={}",
        book_path.display()
    );
    stream_covered_book_positions(connection, book_path)?;
    eprintln!("[corpus] phase=coverage event=corpus_join_start");
    connection.execute_batch(
        "CREATE TEMP TABLE covered_position(
             position_id INTEGER PRIMARY KEY,covered INTEGER NOT NULL
         ) WITHOUT ROWID;
         INSERT INTO covered_position(position_id,covered)
         SELECT DISTINCT p.id,CASE WHEN cb.position_key IS NULL THEN 0 ELSE 1 END
         FROM game_position gp
         JOIN position p ON p.id=gp.position_id
         LEFT JOIN covered_book cb ON cb.position_key=p.position_key;",
    )?;

    let unique = connection.query_row(
        "SELECT COALESCE(SUM(covered),0),COUNT(*) FROM covered_position",
        [],
        |row| Ok(metric(row.get(0)?, row.get(1)?)),
    )?;
    let occurrences = connection.query_row(
        "SELECT COALESCE(SUM(cp.covered),0),COUNT(*)
         FROM game_position gp JOIN covered_position cp ON cp.position_id=gp.position_id",
        [],
        |row| Ok(metric(row.get(0)?, row.get(1)?)),
    )?;
    let revision = connection
        .query_row(
            "SELECT value FROM meta WHERE key='corpus_revision'",
            [],
            |row| row.get::<_, String>(0),
        )?
        .parse::<i64>()
        .map_err(|_| CoverageError::InvalidRevision)?;
    eprintln!("[corpus] phase=coverage event=group_start group=ply_band");
    let mut by_ply_band = group_metrics(connection, "CAST(gp.ply / 20 AS INTEGER)")?;
    by_ply_band = by_ply_band
        .into_iter()
        .map(|(key, value)| {
            let band = key.parse::<i64>().unwrap_or(0);
            (format!("{}-{}", band * 20 + 1, band * 20 + 20), value)
        })
        .collect();
    let report = CoverageReport {
        snapshot_id: snapshot_id.to_owned(),
        book_hash: book_hash.to_owned(),
        corpus_revision: revision,
        overall: GroupCoverage {
            unique,
            occurrences,
        },
        by_site: {
            eprintln!("[corpus] phase=coverage event=group_start group=site");
            group_metrics(connection, "rs.site")?
        },
        by_event: {
            eprintln!("[corpus] phase=coverage event=group_start group=event");
            group_metrics(connection, "rs.event")?
        },
        by_bucket: group_metrics(
            connection,
            "CASE WHEN rs.year >= 2024 THEN (rs.year - 2024) / 3
                  ELSE -((2024 - rs.year + 2) / 3) END",
        )?,
        by_ply_band,
        by_side: group_metrics(
            connection,
            "CASE WHEN gp.ply % 2 = 0 THEN 'black' ELSE 'white' END",
        )?,
    };
    connection.execute_batch("DROP TABLE temp.covered_position; DROP TABLE temp.covered_book;")?;
    Ok(report)
}

fn stream_covered_book_positions(
    connection: &Connection,
    path: &Path,
) -> Result<(), CoverageError> {
    let file = File::open(path).map_err(|source| CoverageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::with_capacity(1024 * 1024, file);
    let mut current_sfen = None;
    let mut scanned_positions = 0_u64;
    let started = Instant::now();
    let mut entries = Vec::new();
    let mut order = 0_usize;
    for line in reader.lines() {
        let line = line.map_err(|source| CoverageError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let line = line.trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if let Some(sfen) = line.strip_prefix("sfen ") {
            scanned_positions += 1;
            if scanned_positions.is_multiple_of(100_000) {
                let elapsed = started.elapsed().as_secs_f64().max(0.001);
                eprintln!(
                    "[corpus] phase=coverage event=book_scan_progress positions={} positions_per_second={:.0}",
                    scanned_positions,
                    scanned_positions as f64 / elapsed
                );
            }
            flush_book_position(connection, current_sfen.take(), &mut entries)?;
            current_sfen = Some(sfen.trim().to_owned());
            continue;
        }
        if current_sfen.is_none() {
            return Err(BookParseError::EntryBeforeSfen(line.to_owned()).into());
        }
        entries.push(parse_book_entry_line(line, order)?);
        order += 1;
    }
    flush_book_position(connection, current_sfen, &mut entries)?;
    eprintln!(
        "[corpus] phase=coverage event=book_scan_done positions={} elapsed_seconds={:.3}",
        scanned_positions,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn flush_book_position(
    connection: &Connection,
    sfen: Option<String>,
    entries: &mut Vec<BookEntry>,
) -> Result<(), rusqlite::Error> {
    let Some(sfen) = sfen else {
        return Ok(());
    };
    if legal_distinct_entry_count(&sfen, entries) > 0 {
        connection.execute(
            "INSERT OR IGNORE INTO covered_book(position_key) VALUES(?1)",
            params![strip_sfen_ply(&sfen)],
        )?;
    }
    entries.clear();
    Ok(())
}

fn group_metrics(
    connection: &Connection,
    expression: &str,
) -> Result<BTreeMap<String, GroupCoverage>, rusqlite::Error> {
    let base = format!(
        "SELECT CAST({expression} AS TEXT) AS group_key,gp.position_id,cp.covered
         FROM game_position gp
         JOIN logical_game lg ON lg.id=gp.game_id
         JOIN raw_source rs ON rs.id=lg.primary_source_id
         JOIN covered_position cp ON cp.position_id=gp.position_id"
    );
    let unique_sql = format!(
        "WITH grouped AS (
             SELECT group_key,position_id,MAX(covered) AS covered
             FROM ({base}) GROUP BY group_key,position_id
         ) SELECT group_key,COALESCE(SUM(covered),0),COUNT(*)
           FROM grouped GROUP BY group_key ORDER BY group_key"
    );
    let occurrence_sql = format!(
        "SELECT group_key,COALESCE(SUM(covered),0),COUNT(*)
         FROM ({base}) GROUP BY group_key ORDER BY group_key"
    );
    let unique = query_group_metrics(connection, &unique_sql)?;
    let occurrences = query_group_metrics(connection, &occurrence_sql)?;
    Ok(unique
        .into_iter()
        .map(|(key, unique)| {
            let occurrences = occurrences
                .get(&key)
                .cloned()
                .unwrap_or_else(|| metric(0, 0));
            (
                key,
                GroupCoverage {
                    unique,
                    occurrences,
                },
            )
        })
        .collect())
}

fn query_group_metrics(
    connection: &Connection,
    sql: &str,
) -> Result<BTreeMap<String, CoverageMetric>, rusqlite::Error> {
    let mut statement = connection.prepare(sql)?;
    statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, metric(row.get(1)?, row.get(2)?)))
        })?
        .collect()
}

fn metric(covered: i64, total: i64) -> CoverageMetric {
    CoverageMetric {
        covered,
        total,
        rate: if total == 0 {
            0.0
        } else {
            covered as f64 / total as f64
        },
    }
}

#[derive(Debug, Error)]
pub enum CoverageError {
    #[error("book I/O failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("book parse failed: {0}")]
    Book(#[from] BookParseError),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("corpus revision is not an integer")]
    InvalidRevision,
}
