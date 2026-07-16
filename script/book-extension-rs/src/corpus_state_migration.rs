use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct MigrationIdentity {
    pub manifest_sha256: String,
    pub profile_sha256: String,
    pub priority_reference_year: i32,
    pub priority_policy_version: String,
    pub ranking_digest: String,
    pub rating_digest: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct MigrationSummary {
    pub source_tasks: i64,
    pub mapped_tasks: i64,
    pub unmapped_tasks: i64,
    pub status_counts: BTreeMap<String, i64>,
    pub copied_checkpoints: i64,
    pub copied_frontier_history: i64,
    pub identity_changed: bool,
}

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("unsupported source schema version {0}")]
    UnsupportedSchema(i64),
    #[error("migration count mismatch: {0}")]
    CountMismatch(String),
}

type TaskRow = (
    String,
    String,
    String,
    String,
    i64,
    Option<f64>,
    Option<String>,
    Option<i32>,
    Option<String>,
    Option<i32>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    Option<i64>,
    f64,
);

pub fn persist_migration_identity(
    connection: &Connection,
    identity: &MigrationIdentity,
) -> Result<(), MigrationError> {
    for (key, value) in identity_pairs(identity) {
        connection.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
    }
    Ok(())
}

pub fn migrate_runtime_state(
    new_db: &Path,
    old_db: &Path,
    identity: &MigrationIdentity,
    now: f64,
) -> Result<MigrationSummary, MigrationError> {
    let old = Connection::open_with_flags(old_db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut new = Connection::open(new_db)?;
    let schema_version = meta_i64(&old, "schema_version")?.unwrap_or(0);
    if !matches!(schema_version, 5 | 6) {
        return Err(MigrationError::UnsupportedSchema(schema_version));
    }
    let identity_changed = identity_pairs(identity).iter().any(|(key, expected)| {
        meta_string(&old, key).ok().flatten().as_deref() != Some(expected.as_str())
    });
    let mut summary = MigrationSummary {
        identity_changed,
        ..MigrationSummary::default()
    };
    let transaction = new.transaction()?;
    let task_sql = if schema_version == 5 {
        "SELECT p.position_key,c.move,t.book_snapshot_id,t.status,t.attempts,t.lease_until,
                t.last_error,t.eval_cp,t.response,t.depth,t.nodes,t.engine_config_id,
                NULL,t.engine_config_id,0,t.persisted_checkpoint_id,t.updated_at
         FROM search_task t
         JOIN candidate c ON c.id=t.candidate_id
         JOIN position p ON p.id=c.position_id
         ORDER BY t.id"
    } else {
        "SELECT p.position_key,c.move,t.book_snapshot_id,t.status,t.attempts,t.lease_until,
                t.last_error,t.eval_cp,t.response,t.depth,t.nodes,t.engine_config_id,
                t.failure_class,t.engine_fingerprint,t.consecutive_engine_failures,
                t.persisted_checkpoint_id,t.updated_at
         FROM search_task t
         JOIN candidate c ON c.id=t.candidate_id
         JOIN position p ON p.id=c.position_id
         ORDER BY t.id"
    };
    let mut referenced_checkpoints = BTreeSet::new();
    {
        let mut statement = old.prepare(task_sql)?;
        let rows = statement.query_map([], |row| -> rusqlite::Result<TaskRow> {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
                row.get(12)?,
                row.get(13)?,
                row.get(14)?,
                row.get(15)?,
                row.get(16)?,
            ))
        })?;
        for row in rows {
            summary.source_tasks += 1;
            let (
                position,
                move_usi,
                snapshot,
                status,
                attempts,
                lease,
                error,
                eval,
                response,
                depth,
                nodes,
                engine_config,
                failure_class,
                fingerprint,
                consecutive,
                checkpoint,
                updated_at,
            ) = row?;
            let candidate_id: Option<i64> = transaction
                .query_row(
                    "SELECT c.id FROM candidate c JOIN position p ON p.id=c.position_id
                     WHERE p.position_key=?1 AND c.move=?2",
                    params![position, move_usi],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(candidate_id) = candidate_id else {
                summary.unmapped_tasks += 1;
                continue;
            };
            let status = if status == "running" {
                "pending"
            } else {
                status.as_str()
            };
            transaction.execute(
                "INSERT INTO search_task(
                     candidate_id,book_snapshot_id,status,attempts,lease_until,last_error,eval_cp,
                     response,depth,nodes,engine_config_id,failure_class,engine_fingerprint,
                     consecutive_engine_failures,persisted_checkpoint_id,updated_at
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
                 ON CONFLICT(candidate_id,book_snapshot_id) DO UPDATE SET
                     status=excluded.status,attempts=excluded.attempts,lease_until=excluded.lease_until,
                     last_error=excluded.last_error,eval_cp=excluded.eval_cp,response=excluded.response,
                     depth=excluded.depth,nodes=excluded.nodes,engine_config_id=excluded.engine_config_id,
                     failure_class=excluded.failure_class,engine_fingerprint=excluded.engine_fingerprint,
                     consecutive_engine_failures=excluded.consecutive_engine_failures,
                     persisted_checkpoint_id=excluded.persisted_checkpoint_id,updated_at=excluded.updated_at",
                params![candidate_id,snapshot,status,attempts,lease,error,eval,response,depth,nodes,
                    engine_config,failure_class,fingerprint,consecutive,checkpoint,updated_at],
            )?;
            if let Some(checkpoint) = checkpoint {
                referenced_checkpoints.insert(checkpoint);
            }
            summary.mapped_tasks += 1;
            *summary.status_counts.entry(status.to_owned()).or_insert(0) += 1;
        }
    }
    if summary.source_tasks != summary.mapped_tasks + summary.unmapped_tasks {
        return Err(MigrationError::CountMismatch(format!(
            "source_tasks={} mapped_tasks={} unmapped_tasks={}",
            summary.source_tasks, summary.mapped_tasks, summary.unmapped_tasks
        )));
    }
    let status_total: i64 = summary.status_counts.values().sum();
    if status_total != summary.mapped_tasks {
        return Err(MigrationError::CountMismatch(format!(
            "mapped_tasks={} status_total={status_total}",
            summary.mapped_tasks
        )));
    }
    for checkpoint_id in referenced_checkpoints {
        let checkpoint: Option<(String, f64)> = old
            .query_row(
                "SELECT book_hash,created_at FROM checkpoint WHERE id=?1",
                [checkpoint_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((book_hash, created_at)) = checkpoint {
            transaction.execute(
                "INSERT OR IGNORE INTO checkpoint(id,book_hash,created_at) VALUES(?1,?2,?3)",
                params![checkpoint_id, book_hash, created_at],
            )?;
            summary.copied_checkpoints += 1;
        }
    }
    {
        let mut statement = old.prepare("SELECT name,value FROM metric_counter")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (name, value) = row?;
            transaction.execute(
                "INSERT INTO metric_counter(name,value) VALUES(?1,?2)
                 ON CONFLICT(name) DO UPDATE SET value=excluded.value",
                params![name, value],
            )?;
        }
    }
    if schema_version == 5 {
        let mut statement = old.prepare(
            "SELECT old_width,new_width,changed_at,reason
             FROM progressive_width_history ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (old_width, new_width, changed_at, reason) = row?;
            transaction.execute(
                "INSERT INTO frontier_history(old_band,new_band,old_width,new_width,changed_at,reason)
                 VALUES(0,0,?1,?2,?3,?4)",
                params![old_width, new_width, changed_at, reason],
            )?;
            summary.copied_frontier_history += 1;
        }
    } else {
        let mut statement = old.prepare(
            "SELECT old_band,new_band,old_width,new_width,changed_at,reason
             FROM frontier_history ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        for row in rows {
            let (old_band, new_band, old_width, new_width, changed_at, reason) = row?;
            transaction.execute(
                "INSERT INTO frontier_history(old_band,new_band,old_width,new_width,changed_at,reason)
                 VALUES(?1,?2,?3,?4,?5,?6)",
                params![old_band, new_band, old_width, new_width, changed_at, reason],
            )?;
            summary.copied_frontier_history += 1;
        }
    }
    let width = meta_i64(&old, "progressive_width")?.unwrap_or(1);
    let frontier = if identity_changed {
        (0, width, 0, 0, -1)
    } else if schema_version == 5 {
        (
            0,
            width,
            meta_i64(&old, "zero_addition_rollouts")?.unwrap_or(0),
            0,
            -1,
        )
    } else {
        (
            meta_i64(&old, "active_quality_band")?.unwrap_or(0),
            width,
            meta_i64(&old, "eligible_miss_count")?.unwrap_or(0),
            meta_i64(&old, "truncated_candidate_seen")?.unwrap_or(0),
            meta_i64(&old, "next_quality_band_seen")?.unwrap_or(-1),
        )
    };
    for (key, value) in [
        ("active_quality_band", frontier.0),
        ("progressive_width", frontier.1),
        ("eligible_miss_count", frontier.2),
        ("truncated_candidate_seen", frontier.3),
        ("next_quality_band_seen", frontier.4),
    ] {
        transaction.execute(
            "UPDATE meta SET value=?1 WHERE key=?2",
            params![value.to_string(), key],
        )?;
    }
    for (key, value) in identity_pairs(identity) {
        transaction.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
    }
    transaction.execute(
        "INSERT INTO frontier_history(old_band,new_band,old_width,new_width,changed_at,reason)
         VALUES(?1,?2,?3,?3,?4,?5)",
        params![
            frontier.0,
            frontier.0,
            frontier.1,
            now,
            if identity_changed {
                "runtime state migrated with identity reset"
            } else {
                "runtime state migrated"
            }
        ],
    )?;
    transaction.commit()?;
    Ok(summary)
}

fn identity_pairs(identity: &MigrationIdentity) -> Vec<(&'static str, String)> {
    vec![
        ("manifest_sha256", identity.manifest_sha256.clone()),
        ("profile_sha256", identity.profile_sha256.clone()),
        (
            "priority_reference_year",
            identity.priority_reference_year.to_string(),
        ),
        (
            "priority_policy_version",
            identity.priority_policy_version.clone(),
        ),
        ("ranking_digest", identity.ranking_digest.clone()),
        ("rating_digest", identity.rating_digest.clone()),
    ]
}

fn meta_string(connection: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    connection
        .query_row("SELECT value FROM meta WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .optional()
}

fn meta_i64(connection: &Connection, key: &str) -> Result<Option<i64>, rusqlite::Error> {
    Ok(meta_string(connection, key)?.and_then(|value| value.parse().ok()))
}
