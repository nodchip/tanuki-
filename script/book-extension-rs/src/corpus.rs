use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use thiserror::Error;

pub const SCHEMA_VERSION: i64 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchTaskStatus {
    Pending,
    Running,
    Evaluated,
    PermanentFailed,
    Superseded,
}

impl SearchTaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Evaluated => "evaluated",
            Self::PermanentFailed => "permanent_failed",
            Self::Superseded => "superseded",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusCandidate {
    pub id: i64,
    pub position_key: String,
    pub move_usi: String,
    pub history: Vec<String>,
    pub source: String,
    pub priority_key: String,
    pub status: SearchTaskStatus,
    pub attempts: i64,
    pub book_snapshot_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchCompletion {
    pub eval_cp: i32,
    pub response: String,
    pub depth: i32,
    pub nodes: i64,
    pub engine_config_id: String,
    pub now: f64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSearchResult {
    pub task_id: i64,
    pub position_key: String,
    pub move_usi: String,
    pub eval_cp: i32,
    pub response: String,
    pub depth: i32,
    pub nodes: i64,
    pub engine_config_id: String,
    pub persisted_checkpoint_id: Option<i64>,
}

#[derive(Debug, Error)]
pub enum CorpusError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database uses newer schema version {0}")]
    NewerSchema(i64),
    #[error("running task not found: {0}")]
    RunningTaskNotFound(i64),
}

pub struct CorpusStore {
    path: PathBuf,
    connection: Connection,
}

impl CorpusStore {
    pub fn open(path: &Path) -> Result<Self, CorpusError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                CorpusError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
            })?;
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        let mut store = Self {
            path: path.to_owned(),
            connection,
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&mut self) -> Result<(), CorpusError> {
        let version: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()
            .or_else(|error| match error {
                rusqlite::Error::SqliteFailure(_, Some(ref message))
                    if message.contains("no such table") =>
                {
                    Ok(None)
                }
                other => Err(other),
            })?;
        if let Some(version) = version {
            let version = version.parse::<i64>().unwrap_or(i64::MAX);
            if version > SCHEMA_VERSION {
                return Err(CorpusError::NewerSchema(version));
            }
        }
        self.connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS candidate (
                id INTEGER PRIMARY KEY,
                position_key TEXT NOT NULL,
                move TEXT NOT NULL,
                history_json TEXT NOT NULL,
                source TEXT NOT NULL,
                priority_key TEXT NOT NULL,
                active INTEGER NOT NULL DEFAULT 1,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL,
                UNIQUE(position_key, move)
            );
            CREATE TABLE IF NOT EXISTS candidate_source (
                candidate_id INTEGER NOT NULL REFERENCES candidate(id),
                source TEXT NOT NULL,
                history_json TEXT NOT NULL,
                PRIMARY KEY(candidate_id, source)
            );
            CREATE INDEX IF NOT EXISTS candidate_priority_idx
                ON candidate(active, priority_key DESC, id);
            CREATE INDEX IF NOT EXISTS candidate_position_priority_idx
                ON candidate(position_key, active, priority_key DESC, id);
            CREATE TABLE IF NOT EXISTS search_task (
                id INTEGER PRIMARY KEY,
                candidate_id INTEGER NOT NULL REFERENCES candidate(id),
                book_snapshot_id TEXT NOT NULL,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                lease_until REAL,
                last_error TEXT,
                eval_cp INTEGER,
                response TEXT,
                depth INTEGER,
                nodes INTEGER,
                engine_config_id TEXT,
                persisted_checkpoint_id INTEGER,
                updated_at REAL NOT NULL,
                UNIQUE(candidate_id, book_snapshot_id)
            );
            CREATE INDEX IF NOT EXISTS search_task_status_idx
                ON search_task(book_snapshot_id, status, lease_until);
            CREATE TABLE IF NOT EXISTS progressive_width_history (
                id INTEGER PRIMARY KEY,
                old_width INTEGER NOT NULL,
                new_width INTEGER NOT NULL,
                changed_at REAL NOT NULL,
                reason TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS metric_counter (
                name TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS checkpoint (
                id INTEGER PRIMARY KEY,
                book_hash TEXT NOT NULL,
                created_at REAL NOT NULL
            );
            ",
        )?;
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES('corpus_revision', '0')",
            [],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES('progressive_width', '1')",
            [],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES('zero_addition_rollouts', '0')",
            [],
        )?;
        transaction.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
            params![SCHEMA_VERSION.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn add_metric(&mut self, name: &str, value: i64) -> Result<(), CorpusError> {
        self.connection.execute(
            "INSERT INTO metric_counter(name, value) VALUES(?1, ?2)
             ON CONFLICT(name) DO UPDATE SET value = metric_counter.value + excluded.value",
            params![name, value],
        )?;
        Ok(())
    }

    pub fn metric(&self, name: &str) -> Result<i64, CorpusError> {
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM metric_counter WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }
    pub fn schema_version(&self) -> Result<i64, CorpusError> {
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )?
            .parse()
            .unwrap_or(i64::MAX))
    }

    pub fn upsert_candidate(
        &mut self,
        position_key: &str,
        move_usi: &str,
        history: &[&str],
        source: &str,
        priority_key: &str,
    ) -> Result<i64, CorpusError> {
        let history_json = serde_json::to_string(history)?;
        let now = unix_time();
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO candidate(position_key, move, history_json, source, priority_key, active, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)
             ON CONFLICT(position_key, move) DO UPDATE SET
               history_json = CASE WHEN excluded.priority_key > candidate.priority_key THEN excluded.history_json ELSE candidate.history_json END,
               source = CASE WHEN excluded.priority_key > candidate.priority_key THEN excluded.source ELSE candidate.source END,
               priority_key = MAX(candidate.priority_key, excluded.priority_key), active = 1, updated_at = excluded.updated_at",
            params![position_key, move_usi, history_json, source, priority_key, now],
        )?;
        let candidate_id: i64 = transaction.query_row(
            "SELECT id FROM candidate WHERE position_key = ?1 AND move = ?2",
            params![position_key, move_usi],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO candidate_source(candidate_id, source, history_json) VALUES(?1, ?2, ?3)
             ON CONFLICT(candidate_id, source) DO UPDATE SET history_json = excluded.history_json",
            params![candidate_id, source, history_json],
        )?;
        transaction.commit()?;
        Ok(candidate_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn reserve_for_position(
        &mut self,
        book_snapshot_id: &str,
        position_key: &str,
        excluded_moves: &HashSet<String>,
        width: usize,
        lease_sec: f64,
        site: Option<&str>,
        now: f64,
    ) -> Result<Option<CorpusCandidate>, CorpusError> {
        if width == 0 {
            return Ok(None);
        }
        let transaction = self.connection.transaction()?;
        let rows = query_candidates(&transaction, book_snapshot_id, position_key, width)?;
        for row in &rows {
            if excluded_moves.contains(&row.move_usi)
                && row.task_id.is_some()
                && row.status.as_deref() == Some(SearchTaskStatus::Pending.as_str())
            {
                transaction.execute(
                    "UPDATE search_task SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![SearchTaskStatus::Superseded.as_str(), now, row.task_id],
                )?;
            }
        }
        let selected = rows.into_iter().find(|row| {
            !excluded_moves.contains(&row.move_usi)
                && site.is_none_or(|expected| {
                    row.source
                        .split_once(':')
                        .map_or(row.source.as_str(), |item| item.0)
                        == expected
                })
                && (row.task_id.is_none()
                    || row.status.as_deref() == Some(SearchTaskStatus::Pending.as_str())
                    || (row.status.as_deref() == Some(SearchTaskStatus::Running.as_str())
                        && row.lease_until.is_some_and(|until| until <= now)))
        });
        let Some(row) = selected else {
            transaction.commit()?;
            return Ok(None);
        };
        let attempts = row.attempts.unwrap_or(0) + 1;
        let task_id = if let Some(task_id) = row.task_id {
            transaction.execute(
                "UPDATE search_task SET status = ?1, attempts = ?2, lease_until = ?3,
                 last_error = NULL, updated_at = ?4 WHERE id = ?5",
                params![
                    SearchTaskStatus::Running.as_str(),
                    attempts,
                    now + lease_sec,
                    now,
                    task_id
                ],
            )?;
            task_id
        } else {
            transaction.execute(
                "INSERT INTO search_task(candidate_id, book_snapshot_id, status, attempts, lease_until, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![row.candidate_id, book_snapshot_id, SearchTaskStatus::Running.as_str(), attempts, now + lease_sec, now],
            )?;
            transaction.last_insert_rowid()
        };
        transaction.commit()?;
        Ok(Some(CorpusCandidate {
            id: task_id,
            position_key: row.position_key,
            move_usi: row.move_usi,
            history: serde_json::from_str(&row.history_json)?,
            source: row.source,
            priority_key: row.priority_key,
            status: SearchTaskStatus::Running,
            attempts,
            book_snapshot_id: book_snapshot_id.to_owned(),
        }))
    }

    pub fn complete_search(
        &mut self,
        task_id: i64,
        completion: &SearchCompletion,
    ) -> Result<(), CorpusError> {
        let changed = self.connection.execute(
            "UPDATE search_task SET status = ?1, lease_until = NULL, last_error = NULL,
             eval_cp = ?2, response = ?3, depth = ?4, nodes = ?5, engine_config_id = ?6,
             persisted_checkpoint_id = NULL, updated_at = ?7 WHERE id = ?8 AND status = ?9",
            params![
                SearchTaskStatus::Evaluated.as_str(),
                completion.eval_cp,
                completion.response,
                completion.depth,
                completion.nodes,
                completion.engine_config_id,
                completion.now,
                task_id,
                SearchTaskStatus::Running.as_str()
            ],
        )?;
        if changed != 1 {
            return Err(CorpusError::RunningTaskNotFound(task_id));
        }
        Ok(())
    }

    pub fn unpersisted_results(&self) -> Result<Vec<StoredSearchResult>, CorpusError> {
        self.query_results(
            "t.status = ?1 AND t.persisted_checkpoint_id IS NULL",
            &[&SearchTaskStatus::Evaluated.as_str()],
        )
    }

    pub fn results_requiring_replay(
        &self,
        book_hash: &str,
    ) -> Result<Vec<StoredSearchResult>, CorpusError> {
        let checkpoint: Option<i64> = self.connection.query_row(
            "SELECT MAX(id) FROM checkpoint WHERE book_hash = ?1",
            params![book_hash],
            |row| row.get(0),
        )?;
        if let Some(checkpoint) = checkpoint {
            self.query_results(
                "t.status = ?1 AND (t.persisted_checkpoint_id IS NULL OR t.persisted_checkpoint_id > ?2)",
                &[&SearchTaskStatus::Evaluated.as_str(), &checkpoint],
            )
        } else {
            self.query_results("t.status = ?1", &[&SearchTaskStatus::Evaluated.as_str()])
        }
    }

    fn query_results(
        &self,
        condition: &str,
        parameters: &[&dyn rusqlite::ToSql],
    ) -> Result<Vec<StoredSearchResult>, CorpusError> {
        let sql = format!(
            "SELECT t.id, c.position_key, c.move, t.eval_cp, t.response, t.depth, t.nodes,
             t.engine_config_id, t.persisted_checkpoint_id FROM search_task t
             JOIN candidate c ON c.id = t.candidate_id WHERE {condition} ORDER BY t.id"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(parameters, |row| {
            Ok(StoredSearchResult {
                task_id: row.get(0)?,
                position_key: row.get(1)?,
                move_usi: row.get(2)?,
                eval_cp: row.get(3)?,
                response: row.get(4)?,
                depth: row.get(5)?,
                nodes: row.get(6)?,
                engine_config_id: row.get(7)?,
                persisted_checkpoint_id: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn has_checkpoint(&self, book_hash: &str) -> Result<bool, CorpusError> {
        Ok(self
            .connection
            .query_row(
                "SELECT 1 FROM checkpoint WHERE book_hash = ?1 LIMIT 1",
                params![book_hash],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
    pub fn fail_search(
        &mut self,
        task_id: i64,
        error: &str,
        max_attempts: i64,
        now: f64,
    ) -> Result<SearchTaskStatus, CorpusError> {
        let attempts: i64 = self.connection.query_row(
            "SELECT attempts FROM search_task WHERE id = ?1",
            params![task_id],
            |row| row.get(0),
        )?;
        let status = if attempts >= max_attempts {
            SearchTaskStatus::PermanentFailed
        } else {
            SearchTaskStatus::Pending
        };
        self.connection.execute(
            "UPDATE search_task SET status = ?1, lease_until = NULL, last_error = ?2, updated_at = ?3 WHERE id = ?4",
            params![status.as_str(), error, now, task_id],
        )?;
        Ok(status)
    }

    pub fn reset_interrupted_tasks(&mut self, now: f64) -> Result<usize, CorpusError> {
        Ok(self.connection.execute(
            "UPDATE search_task SET status = ?1, lease_until = NULL, updated_at = ?2 WHERE status = ?3",
            params![SearchTaskStatus::Pending.as_str(), now, SearchTaskStatus::Running.as_str()],
        )?)
    }

    pub fn progressive_width(&self) -> Result<i64, CorpusError> {
        self.meta_int("progressive_width")
    }
    pub fn zero_addition_rollouts(&self) -> Result<i64, CorpusError> {
        self.meta_int("zero_addition_rollouts")
    }
    pub fn corpus_revision(&self) -> Result<i64, CorpusError> {
        self.meta_int("corpus_revision")
    }

    fn meta_int(&self, key: &str) -> Result<i64, CorpusError> {
        let value: String = self.connection.query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )?;
        Ok(value.parse().unwrap_or(i64::MAX))
    }

    pub fn record_corpus_rollout(
        &mut self,
        added: bool,
        saturation_window: i64,
        now: f64,
    ) -> Result<bool, CorpusError> {
        let zero_count = if added {
            0
        } else {
            self.zero_addition_rollouts()? + 1
        };
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'zero_addition_rollouts'",
            params![zero_count.to_string()],
        )?;
        let pending: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM search_task WHERE status IN (?1, ?2)",
            params![
                SearchTaskStatus::Pending.as_str(),
                SearchTaskStatus::Running.as_str()
            ],
            |row| row.get(0),
        )?;
        if zero_count < saturation_window || pending != 0 {
            transaction.commit()?;
            return Ok(false);
        }
        let old_width: i64 = transaction
            .query_row(
                "SELECT value FROM meta WHERE key = 'progressive_width'",
                [],
                |row| row.get::<_, String>(0),
            )?
            .parse()
            .unwrap_or(i64::MAX);
        let new_width = old_width + 1;
        transaction.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'progressive_width'",
            params![new_width.to_string()],
        )?;
        transaction.execute(
            "INSERT INTO progressive_width_history(old_width, new_width, changed_at, reason) VALUES(?1, ?2, ?3, ?4)",
            params![old_width, new_width, now, format!("{saturation_window} zero-addition rollouts")],
        )?;
        transaction.execute(
            "UPDATE meta SET value = '0' WHERE key = 'zero_addition_rollouts'",
            [],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn bump_corpus_revision(&mut self) -> Result<i64, CorpusError> {
        let revision = self.corpus_revision()? + 1;
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'corpus_revision'",
            params![revision.to_string()],
        )?;
        transaction.execute(
            "UPDATE meta SET value = '0' WHERE key = 'zero_addition_rollouts'",
            [],
        )?;
        transaction.commit()?;
        Ok(revision)
    }
    pub fn latest_evaluated_task_id(&self) -> Result<Option<i64>, CorpusError> {
        Ok(self.connection.query_row(
            "SELECT MAX(id) FROM search_task WHERE status = ?1",
            params![SearchTaskStatus::Evaluated.as_str()],
            |row| row.get(0),
        )?)
    }

    pub fn record_checkpoint_through(
        &mut self,
        book_hash: &str,
        through_task_id: Option<i64>,
        now: f64,
    ) -> Result<i64, CorpusError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO checkpoint(book_hash, created_at) VALUES(?1, ?2)",
            params![book_hash, now],
        )?;
        let checkpoint_id = transaction.last_insert_rowid();
        if let Some(through_task_id) = through_task_id {
            transaction.execute(
                "UPDATE search_task SET persisted_checkpoint_id = ?1, updated_at = ?2
                 WHERE status = ?3 AND persisted_checkpoint_id IS NULL AND id <= ?4",
                params![
                    checkpoint_id,
                    now,
                    SearchTaskStatus::Evaluated.as_str(),
                    through_task_id
                ],
            )?;
        }
        transaction.commit()?;
        Ok(checkpoint_id)
    }
    pub fn record_checkpoint(&mut self, book_hash: &str, now: f64) -> Result<i64, CorpusError> {
        let through = self.latest_evaluated_task_id()?;
        self.record_checkpoint_through(book_hash, through, now)
    }
    pub fn position_query_plan(
        &self,
        book_snapshot_id: &str,
        position_key: &str,
        width: usize,
    ) -> Result<Vec<String>, CorpusError> {
        let sql = format!("EXPLAIN QUERY PLAN {}", POSITION_QUERY);
        let mut statement = self.connection.prepare(&sql)?;
        let lines = statement.query_map(
            params![book_snapshot_id, position_key, width as i64],
            |row| row.get(3),
        )?;
        Ok(lines.collect::<Result<Vec<_>, _>>()?)
    }
}

const POSITION_QUERY: &str = "
SELECT c.id, c.position_key, c.move, c.history_json, c.source, c.priority_key,
       t.id, t.status, t.attempts, t.lease_until
FROM candidate c LEFT JOIN search_task t
  ON t.candidate_id = c.id AND t.book_snapshot_id = ?1
WHERE c.active = 1 AND c.position_key = ?2
ORDER BY c.priority_key DESC, c.id LIMIT ?3";

struct CandidateRow {
    candidate_id: i64,
    position_key: String,
    move_usi: String,
    history_json: String,
    source: String,
    priority_key: String,
    task_id: Option<i64>,
    status: Option<String>,
    attempts: Option<i64>,
    lease_until: Option<f64>,
}

fn query_candidates(
    transaction: &Transaction<'_>,
    book_snapshot_id: &str,
    position_key: &str,
    width: usize,
) -> Result<Vec<CandidateRow>, rusqlite::Error> {
    let mut statement = transaction.prepare(POSITION_QUERY)?;
    let rows = statement.query_map(
        params![book_snapshot_id, position_key, width as i64],
        |row| {
            Ok(CandidateRow {
                candidate_id: row.get(0)?,
                position_key: row.get(1)?,
                move_usi: row.get(2)?,
                history_json: row.get(3)?,
                source: row.get(4)?,
                priority_key: row.get(5)?,
                task_id: row.get(6)?,
                status: row.get(7)?,
                attempts: row.get(8)?,
                lease_until: row.get(9)?,
            })
        },
    )?;
    rows.collect()
}

fn unix_time() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
