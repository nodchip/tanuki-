use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use thiserror::Error;

pub const SCHEMA_VERSION: i64 = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum SourceSite {
    Wcsc = 0,
    Denryu = 1,
    Floodgate = 2,
    Unknown = 3,
}

impl SourceSite {
    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "wcsc" => Self::Wcsc,
            "denryu" => Self::Denryu,
            "floodgate" => Self::Floodgate,
            _ => Self::Unknown,
        }
    }

    fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::Wcsc,
            1 => Self::Denryu,
            2 => Self::Floodgate,
            _ => Self::Unknown,
        }
    }
}

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

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "evaluated" => Some(Self::Evaluated),
            "permanent_failed" => Some(Self::PermanentFailed),
            "superseded" => Some(Self::Superseded),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureClass {
    EngineRetryExhausted,
    InvalidCandidate,
    HistoryUnavailable,
}

impl FailureClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EngineRetryExhausted => "engine_retry_exhausted",
            Self::InvalidCandidate => "invalid_candidate",
            Self::HistoryUnavailable => "history_unavailable",
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
    pub priority_key: Vec<u8>,
    pub status: SearchTaskStatus,
    pub attempts: i64,
    pub book_snapshot_id: String,
}

#[derive(Clone, Debug)]
pub struct CandidateChoice {
    pub candidate_id: i64,
    pub position_key: String,
    pub move_usi: String,
    pub history: Vec<String>,
    pub source: String,
    pub source_site: SourceSite,
    pub quality_band: i32,
    pub priority_key: Vec<u8>,
    pub observed_task_id: Option<i64>,
    pub observed_status: Option<SearchTaskStatus>,
    pub observed_lease_until: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct CandidateProbe {
    pub choices: Vec<CandidateChoice>,
    pub truncated: bool,
    pub next_quality_band: Option<i32>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontierState {
    pub active_quality_band: i32,
    pub progressive_width: i32,
    pub eligible_miss_count: i64,
    pub truncated_candidate_seen: bool,
    pub next_quality_band_seen: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RolloutObservation {
    pub eligible_miss: bool,
    pub reserved: bool,
    pub truncated_in_active_band: bool,
    pub smallest_higher_band: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontierTransition {
    None,
    Width { old: i32, new: i32 },
    Band { old: i32, new: i32 },
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
    #[error("database schema version {0} must be rebuilt as version 6 with prepare_book_corpus.py")]
    OlderSchema(i64),
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
            if version < SCHEMA_VERSION {
                return Err(CorpusError::OlderSchema(version));
            }
        }
        self.connection
            .execute_batch(include_str!("../../corpus_schema.sql"))?;
        let transaction = self.connection.transaction()?;
        for (key, value) in [
            ("schema_version", SCHEMA_VERSION.to_string()),
            ("corpus_revision", "0".to_owned()),
            ("active_quality_band", "0".to_owned()),
            ("progressive_width", "1".to_owned()),
            ("eligible_miss_count", "0".to_owned()),
            ("truncated_candidate_seen", "0".to_owned()),
            ("next_quality_band_seen", "-1".to_owned()),
            ("priority_reference_year", "0".to_owned()),
            (
                "priority_policy_version",
                crate::priority::POLICY_VERSION.to_owned(),
            ),
        ] {
            transaction.execute(
                "INSERT OR IGNORE INTO meta(key, value) VALUES(?1, ?2)",
                params![key, value],
            )?;
        }
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
    pub fn task_status_counts(
        &self,
    ) -> Result<crate::runtime_status::TaskStatusCounts, CorpusError> {
        let mut counts = crate::runtime_status::TaskStatusCounts::default();
        let mut statement = self
            .connection
            .prepare("SELECT status,COUNT(*) FROM search_task GROUP BY status")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (status, count) = row?;
            match status.as_str() {
                "pending" => counts.pending = count,
                "running" => counts.running = count,
                "evaluated" => counts.evaluated = count,
                "permanent_failed" => counts.permanent_failed = count,
                _ => {}
            }
        }
        Ok(counts)
    }

    pub fn site_node_metrics(
        &self,
    ) -> Result<std::collections::BTreeMap<String, i64>, CorpusError> {
        let mut metrics = std::collections::BTreeMap::new();
        let mut statement = self.connection.prepare(
            "SELECT name,value FROM metric_counter WHERE name LIKE 'corpus_nodes:%' ORDER BY name",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (name, value) = row?;
            metrics.insert(name.trim_start_matches("corpus_nodes:").to_owned(), value);
        }
        Ok(metrics)
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
        let canonical = canonical_position_key(position_key);
        let now = unix_time();
        let priority_blob = format!("{priority_key:0>88}").into_bytes();
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO position(position_key) VALUES(?1)",
            params![canonical],
        )?;
        let position_id: i64 = transaction.query_row(
            "SELECT id FROM position WHERE position_key = ?1",
            params![canonical],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO candidate(
                 position_id, move, priority_key, quality_band, source_site,
                 active, created_at, updated_at
             )
             VALUES(?1, ?2, ?3, 4, ?4, 1, ?5, ?5)
             ON CONFLICT(position_id, move) DO UPDATE SET
               priority_key = MAX(candidate.priority_key, excluded.priority_key),
               active = 1, updated_at = excluded.updated_at",
            params![
                position_id,
                move_usi,
                priority_blob,
                SourceSite::Unknown as i32,
                now
            ],
        )?;
        let candidate_id: i64 = transaction.query_row(
            "SELECT id FROM candidate WHERE position_id = ?1 AND move = ?2",
            params![position_id, move_usi],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO candidate_adhoc_history(candidate_id, position_sfen, history_json, source)
             VALUES(?1, ?2, ?3, ?4) ON CONFLICT(candidate_id) DO UPDATE SET
             position_sfen=excluded.position_sfen,
             history_json=excluded.history_json, source=excluded.source",
            params![candidate_id, position_key, history_json, source],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO candidate_adhoc_source(candidate_id, source) VALUES(?1, ?2)",
            params![candidate_id, source],
        )?;
        transaction.commit()?;
        Ok(candidate_id)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn probe_position(
        &self,
        book_snapshot_id: &str,
        position_key: &str,
        excluded_moves: &HashSet<String>,
        active_quality_band: i32,
        source_site: SourceSite,
        width: usize,
        now: f64,
    ) -> Result<CandidateProbe, CorpusError> {
        let next_quality_band = self.connection.query_row(
            "SELECT MIN(c.quality_band) FROM candidate c
                 JOIN position p ON p.id=c.position_id
                 WHERE c.active=1 AND p.position_key=?1 AND c.quality_band>?2",
            params![canonical_position_key(position_key), active_quality_band],
            |row| row.get(0),
        )?;
        if width == 0 {
            return Ok(CandidateProbe {
                choices: Vec::new(),
                truncated: false,
                next_quality_band,
            });
        }
        let mut rows = query_filtered_candidates(
            &self.connection,
            book_snapshot_id,
            position_key,
            active_quality_band,
            source_site,
            width + 1,
        )?;
        let truncated = rows.len() > width;
        rows.truncate(width);
        for row in &rows {
            if excluded_moves.contains(&row.move_usi)
                && row.task_id.is_some()
                && row.status.as_deref() == Some(SearchTaskStatus::Pending.as_str())
            {
                self.connection.execute(
                    "UPDATE search_task SET status=?1,updated_at=?2
                     WHERE id=?3 AND status=?4",
                    params![
                        SearchTaskStatus::Superseded.as_str(),
                        now,
                        row.task_id,
                        SearchTaskStatus::Pending.as_str(),
                    ],
                )?;
            }
        }
        let choices = rows
            .into_iter()
            .filter(|row| {
                !excluded_moves.contains(&row.move_usi)
                    && (row.task_id.is_none()
                        || row.status.as_deref() == Some(SearchTaskStatus::Pending.as_str())
                        || (row.status.as_deref() == Some(SearchTaskStatus::Running.as_str())
                            && row.lease_until.is_some_and(|until| until <= now)))
            })
            .map(|row| CandidateChoice {
                candidate_id: row.candidate_id,
                position_key: row.position_key,
                move_usi: row.move_usi,
                history: row.history,
                source: row.source,
                source_site: row.source_site,
                quality_band: row.quality_band,
                priority_key: row.priority_key,
                observed_task_id: row.task_id,
                observed_status: row.status.as_deref().and_then(SearchTaskStatus::from_str),
                observed_lease_until: row.lease_until,
            })
            .collect::<Vec<_>>();
        Ok(CandidateProbe {
            choices,
            truncated,
            next_quality_band,
        })
    }

    pub fn reserve_choice(
        &mut self,
        choice: &CandidateChoice,
        book_snapshot_id: &str,
        lease_sec: f64,
        now: f64,
    ) -> Result<Option<CorpusCandidate>, CorpusError> {
        let transaction = self.connection.transaction()?;
        let task_id = if let Some(task_id) = choice.observed_task_id {
            let Some(status) = choice.observed_status else {
                transaction.commit()?;
                return Ok(None);
            };
            let reservable = status == SearchTaskStatus::Pending
                || (status == SearchTaskStatus::Running
                    && choice
                        .observed_lease_until
                        .is_some_and(|until| until <= now));
            if !reservable {
                transaction.commit()?;
                return Ok(None);
            }
            let changed = transaction.execute(
                "UPDATE search_task SET status=?1,attempts=attempts+1,lease_until=?2,
                    last_error=NULL,updated_at=?3
                 WHERE id=?4 AND candidate_id=?5 AND book_snapshot_id=?6
                   AND status=?7 AND lease_until IS ?8",
                params![
                    SearchTaskStatus::Running.as_str(),
                    now + lease_sec,
                    now,
                    task_id,
                    choice.candidate_id,
                    book_snapshot_id,
                    status.as_str(),
                    choice.observed_lease_until,
                ],
            )?;
            if changed != 1 {
                transaction.commit()?;
                return Ok(None);
            }
            task_id
        } else {
            let changed = transaction.execute(
                "INSERT OR IGNORE INTO search_task(
                     candidate_id,book_snapshot_id,status,attempts,lease_until,updated_at
                 ) VALUES(?1,?2,?3,1,?4,?5)",
                params![
                    choice.candidate_id,
                    book_snapshot_id,
                    SearchTaskStatus::Running.as_str(),
                    now + lease_sec,
                    now,
                ],
            )?;
            if changed != 1 {
                transaction.commit()?;
                return Ok(None);
            }
            transaction.last_insert_rowid()
        };
        let attempts: i64 = transaction.query_row(
            "SELECT attempts FROM search_task WHERE id=?1",
            [task_id],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(Some(CorpusCandidate {
            id: task_id,
            position_key: position_sfen(&choice.position_key),
            move_usi: choice.move_usi.clone(),
            history: choice.history.clone(),
            source: choice.source.clone(),
            priority_key: choice.priority_key.clone(),
            status: SearchTaskStatus::Running,
            attempts,
            book_snapshot_id: book_snapshot_id.to_owned(),
        }))
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
            position_key: position_sfen(&row.position_key),
            move_usi: row.move_usi,
            history: row.history,
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
             engine_fingerprint = ?6, failure_class = NULL, consecutive_engine_failures = 0,
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
            "SELECT t.id, COALESCE(ah.position_sfen, p.position_key || ' 0'), c.move,
             t.eval_cp, t.response, t.depth, t.nodes, t.engine_config_id,
             t.persisted_checkpoint_id FROM search_task t
             JOIN candidate c ON c.id = t.candidate_id
             JOIN position p ON p.id = c.position_id
             LEFT JOIN candidate_adhoc_history ah ON ah.candidate_id = c.id
             WHERE {condition} ORDER BY t.id"
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
    pub fn fail_engine_search(
        &mut self,
        task_id: i64,
        error: &str,
        engine_fingerprint: &str,
        now: f64,
    ) -> Result<SearchTaskStatus, CorpusError> {
        let consecutive: i64 = self.connection.query_row(
            "SELECT consecutive_engine_failures FROM search_task WHERE id=?1 AND status=?2",
            params![task_id, SearchTaskStatus::Running.as_str()],
            |row| row.get(0),
        )?;
        let consecutive = consecutive + 1;
        let status = if consecutive >= 3 {
            SearchTaskStatus::PermanentFailed
        } else {
            SearchTaskStatus::Pending
        };
        let failure_class = (status == SearchTaskStatus::PermanentFailed)
            .then_some(FailureClass::EngineRetryExhausted.as_str());
        self.connection.execute(
            "UPDATE search_task SET status=?1,lease_until=NULL,last_error=?2,
                failure_class=?3,engine_fingerprint=?4,consecutive_engine_failures=?5,updated_at=?6
             WHERE id=?7 AND status=?8",
            params![
                status.as_str(),
                error,
                failure_class,
                engine_fingerprint,
                consecutive,
                now,
                task_id,
                SearchTaskStatus::Running.as_str(),
            ],
        )?;
        Ok(status)
    }

    pub fn fail_deterministic(
        &mut self,
        task_id: i64,
        failure_class: FailureClass,
        error: &str,
        engine_fingerprint: &str,
        now: f64,
    ) -> Result<SearchTaskStatus, CorpusError> {
        self.connection.execute(
            "UPDATE search_task SET status=?1,lease_until=NULL,last_error=?2,
                failure_class=?3,engine_fingerprint=?4,updated_at=?5
             WHERE id=?6 AND status=?7",
            params![
                SearchTaskStatus::PermanentFailed.as_str(),
                error,
                failure_class.as_str(),
                engine_fingerprint,
                now,
                task_id,
                SearchTaskStatus::Running.as_str(),
            ],
        )?;
        Ok(SearchTaskStatus::PermanentFailed)
    }

    pub fn interrupt_search(&mut self, task_id: i64, now: f64) -> Result<(), CorpusError> {
        let changed = self.connection.execute(
            "UPDATE search_task SET status=?1,lease_until=NULL,updated_at=?2
             WHERE id=?3 AND status=?4",
            params![
                SearchTaskStatus::Pending.as_str(),
                now,
                task_id,
                SearchTaskStatus::Running.as_str(),
            ],
        )?;
        if changed != 1 {
            return Err(CorpusError::RunningTaskNotFound(task_id));
        }
        Ok(())
    }

    pub fn requeue_retryable_failures(
        &mut self,
        engine_fingerprint: &str,
        now: f64,
    ) -> Result<usize, CorpusError> {
        Ok(self.connection.execute(
            "UPDATE search_task SET status=?1,failure_class=NULL,last_error=NULL,
                consecutive_engine_failures=0,updated_at=?2
             WHERE status=?3 AND failure_class=?4
               AND COALESCE(engine_fingerprint,'')<>?5",
            params![
                SearchTaskStatus::Pending.as_str(),
                now,
                SearchTaskStatus::PermanentFailed.as_str(),
                FailureClass::EngineRetryExhausted.as_str(),
                engine_fingerprint,
            ],
        )?)
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
        self.meta_int("eligible_miss_count")
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

    pub fn frontier_state(&self) -> Result<FrontierState, CorpusError> {
        frontier_state_from_connection(&self.connection)
    }

    pub fn record_rollout_observation(
        &mut self,
        observation: &RolloutObservation,
        saturation_window: i64,
        now: f64,
    ) -> Result<FrontierTransition, CorpusError> {
        let saturation_window = saturation_window.max(1);
        let transaction = self.connection.transaction()?;
        let mut state = frontier_state_from_transaction(&transaction)?;
        state.truncated_candidate_seen |= observation.truncated_in_active_band;
        if let Some(band) = observation
            .smallest_higher_band
            .filter(|band| *band > state.active_quality_band)
        {
            state.next_quality_band_seen = Some(
                state
                    .next_quality_band_seen
                    .map_or(band, |current| current.min(band)),
            );
        }
        if observation.reserved {
            state.eligible_miss_count = 0;
        } else if observation.eligible_miss {
            state.eligible_miss_count += 1;
        }
        write_frontier_state(&transaction, &state)?;
        if observation.reserved
            || !observation.eligible_miss
            || state.eligible_miss_count < saturation_window
        {
            transaction.commit()?;
            return Ok(FrontierTransition::None);
        }
        let runnable_tasks: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM search_task t
             JOIN candidate c ON c.id=t.candidate_id
             WHERE t.status IN (?1,?2) AND c.active=1 AND c.quality_band<=?3",
            params![
                SearchTaskStatus::Pending.as_str(),
                SearchTaskStatus::Running.as_str(),
                state.active_quality_band,
            ],
            |row| row.get(0),
        )?;
        if runnable_tasks != 0 {
            transaction.commit()?;
            return Ok(FrontierTransition::None);
        }
        let old_band = state.active_quality_band;
        let old_width = state.progressive_width;
        let transition = if state.truncated_candidate_seen {
            state.progressive_width += 1;
            FrontierTransition::Width {
                old: old_width,
                new: state.progressive_width,
            }
        } else if let Some(next_band) = state.next_quality_band_seen {
            state.active_quality_band = next_band;
            FrontierTransition::Band {
                old: old_band,
                new: next_band,
            }
        } else {
            FrontierTransition::None
        };
        if transition == FrontierTransition::None {
            transaction.commit()?;
            return Ok(transition);
        }
        state.eligible_miss_count = 0;
        state.truncated_candidate_seen = false;
        state.next_quality_band_seen = None;
        write_frontier_state(&transaction, &state)?;
        transaction.execute(
            "INSERT INTO frontier_history(
                 old_band,new_band,old_width,new_width,changed_at,reason
             ) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                old_band,
                state.active_quality_band,
                old_width,
                state.progressive_width,
                now,
                format!("{saturation_window} eligible misses"),
            ],
        )?;
        transaction.commit()?;
        Ok(transition)
    }

    pub fn reset_frontier_for_revision(
        &mut self,
        now: f64,
        reason: &str,
    ) -> Result<(), CorpusError> {
        let transaction = self.connection.transaction()?;
        let old = frontier_state_from_transaction(&transaction)?;
        let reset = FrontierState {
            active_quality_band: 0,
            progressive_width: old.progressive_width,
            eligible_miss_count: 0,
            truncated_candidate_seen: false,
            next_quality_band_seen: None,
        };
        write_frontier_state(&transaction, &reset)?;
        transaction.execute(
            "INSERT INTO frontier_history(
                 old_band,new_band,old_width,new_width,changed_at,reason
             ) VALUES(?1,0,?2,?2,?3,?4)",
            params![old.active_quality_band, old.progressive_width, now, reason],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn record_corpus_rollout(
        &mut self,
        added: bool,
        saturation_window: i64,
        now: f64,
    ) -> Result<bool, CorpusError> {
        Ok(self.record_rollout_observation(
            &RolloutObservation {
                eligible_miss: !added,
                reserved: added,
                truncated_in_active_band: !added,
                smallest_higher_band: None,
            },
            saturation_window,
            now,
        )? != FrontierTransition::None)
    }
    pub fn bump_corpus_revision(&mut self) -> Result<i64, CorpusError> {
        let transaction = self.connection.transaction()?;
        let revision = transaction
            .query_row(
                "SELECT value FROM meta WHERE key='corpus_revision'",
                [],
                |row| row.get::<_, String>(0),
            )?
            .parse::<i64>()
            .unwrap_or(i64::MAX)
            + 1;
        let old = frontier_state_from_transaction(&transaction)?;
        let reset = FrontierState {
            active_quality_band: 0,
            progressive_width: old.progressive_width,
            eligible_miss_count: 0,
            truncated_candidate_seen: false,
            next_quality_band_seen: None,
        };
        transaction.execute(
            "UPDATE meta SET value=?1 WHERE key='corpus_revision'",
            [revision.to_string()],
        )?;
        write_frontier_state(&transaction, &reset)?;
        transaction.execute(
            "INSERT INTO frontier_history(
                 old_band,new_band,old_width,new_width,changed_at,reason
             ) VALUES(?1,0,?2,?2,?3,'corpus revision changed')",
            params![old.active_quality_band, old.progressive_width, unix_time()],
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
        active_quality_band: i32,
        source_site: SourceSite,
        width: usize,
    ) -> Result<Vec<String>, CorpusError> {
        let sql = format!("EXPLAIN QUERY PLAN {}", POSITION_FILTERED_QUERY);
        let mut statement = self.connection.prepare(&sql)?;
        let lines = statement.query_map(
            params![
                book_snapshot_id,
                canonical_position_key(position_key),
                source_site as i32,
                active_quality_band,
                width as i64
            ],
            |row| row.get(3),
        )?;
        Ok(lines.collect::<Result<Vec<_>, _>>()?)
    }
}

fn frontier_state_from_connection(connection: &Connection) -> Result<FrontierState, CorpusError> {
    let read = |key: &str| -> Result<i64, CorpusError> {
        let value: String =
            connection.query_row("SELECT value FROM meta WHERE key=?1", [key], |row| {
                row.get(0)
            })?;
        Ok(value.parse().unwrap_or(i64::MAX))
    };
    let next = read("next_quality_band_seen")? as i32;
    Ok(FrontierState {
        active_quality_band: read("active_quality_band")? as i32,
        progressive_width: read("progressive_width")? as i32,
        eligible_miss_count: read("eligible_miss_count")?,
        truncated_candidate_seen: read("truncated_candidate_seen")? != 0,
        next_quality_band_seen: (next >= 0).then_some(next),
    })
}

fn frontier_state_from_transaction(
    transaction: &Transaction<'_>,
) -> Result<FrontierState, CorpusError> {
    frontier_state_from_connection(transaction)
}

fn write_frontier_state(
    transaction: &Transaction<'_>,
    state: &FrontierState,
) -> Result<(), CorpusError> {
    for (key, value) in [
        ("active_quality_band", state.active_quality_band.to_string()),
        ("progressive_width", state.progressive_width.to_string()),
        ("eligible_miss_count", state.eligible_miss_count.to_string()),
        (
            "truncated_candidate_seen",
            i32::from(state.truncated_candidate_seen).to_string(),
        ),
        (
            "next_quality_band_seen",
            state.next_quality_band_seen.unwrap_or(-1).to_string(),
        ),
    ] {
        transaction.execute("UPDATE meta SET value=?1 WHERE key=?2", params![value, key])?;
    }
    Ok(())
}
const POSITION_FILTERED_QUERY: &str = "
SELECT c.id
FROM candidate c
JOIN position p ON p.id = c.position_id
LEFT JOIN search_task t ON t.candidate_id = c.id AND t.book_snapshot_id = ?1
WHERE p.position_key = ?2 AND c.active = 1 AND c.source_site = ?3
  AND c.quality_band <= ?4
ORDER BY c.quality_band ASC, c.priority_key DESC, c.id LIMIT ?5";

const POSITION_QUERY: &str = "
SELECT c.id, p.position_key, c.move, c.priority_key, c.source_site, c.quality_band,
       c.representative_ply, lg.moves_json, rs.site, rs.event, rs.relative_path,
       ah.history_json, ah.source, t.id, t.status, t.attempts, t.lease_until
FROM candidate c
JOIN position p ON p.id = c.position_id
LEFT JOIN logical_game lg ON lg.id = c.representative_game_id
LEFT JOIN raw_source rs ON rs.id = c.source_id
LEFT JOIN candidate_adhoc_history ah ON ah.candidate_id = c.id
LEFT JOIN search_task t ON t.candidate_id = c.id AND t.book_snapshot_id = ?1
WHERE c.active = 1 AND p.position_key = ?2
ORDER BY c.priority_key DESC, c.id LIMIT ?3";

const POSITION_PROBE_QUERY: &str = "
SELECT c.id, p.position_key, c.move, c.priority_key, c.source_site, c.quality_band,
       c.representative_ply, lg.moves_json, rs.site, rs.event, rs.relative_path,
       ah.history_json, ah.source, t.id, t.status, t.attempts, t.lease_until
FROM candidate c
JOIN position p ON p.id = c.position_id
LEFT JOIN logical_game lg ON lg.id = c.representative_game_id
LEFT JOIN raw_source rs ON rs.id = c.source_id
LEFT JOIN candidate_adhoc_history ah ON ah.candidate_id = c.id
LEFT JOIN search_task t ON t.candidate_id = c.id AND t.book_snapshot_id = ?1
WHERE c.active = 1 AND p.position_key = ?2 AND c.source_site = ?3
  AND c.quality_band <= ?4
ORDER BY c.quality_band ASC, c.priority_key DESC, c.id LIMIT ?5";

struct CandidateRow {
    candidate_id: i64,
    position_key: String,
    move_usi: String,
    history: Vec<String>,
    source: String,
    source_site: SourceSite,
    quality_band: i32,
    priority_key: Vec<u8>,
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
) -> Result<Vec<CandidateRow>, CorpusError> {
    let mut statement = transaction.prepare(POSITION_QUERY)?;
    collect_candidate_rows(
        &mut statement,
        params![
            book_snapshot_id,
            canonical_position_key(position_key),
            width as i64
        ],
    )
}

fn query_filtered_candidates(
    connection: &Connection,
    book_snapshot_id: &str,
    position_key: &str,
    active_quality_band: i32,
    source_site: SourceSite,
    width: usize,
) -> Result<Vec<CandidateRow>, CorpusError> {
    let mut statement = connection.prepare(POSITION_PROBE_QUERY)?;
    collect_candidate_rows(
        &mut statement,
        params![
            book_snapshot_id,
            canonical_position_key(position_key),
            source_site as i32,
            active_quality_band,
            width as i64,
        ],
    )
}

fn collect_candidate_rows<P>(
    statement: &mut rusqlite::Statement<'_>,
    parameters: P,
) -> Result<Vec<CandidateRow>, CorpusError>
where
    P: rusqlite::Params,
{
    let rows = statement.query_map(parameters, |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Vec<u8>>(3)?,
            row.get::<_, i32>(4)?,
            row.get::<_, i32>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<i64>>(13)?,
            row.get::<_, Option<String>>(14)?,
            row.get::<_, Option<i64>>(15)?,
            row.get::<_, Option<f64>>(16)?,
        ))
    })?;
    let mut candidates = Vec::new();
    for row in rows {
        let (
            candidate_id,
            position_key,
            move_usi,
            priority_key,
            source_site,
            quality_band,
            representative_ply,
            moves_json,
            site,
            event,
            relative_path,
            adhoc_history,
            adhoc_source,
            task_id,
            status,
            attempts,
            lease_until,
        ) = row?;
        let history = if let Some(moves_json) = moves_json {
            let mut moves: Vec<String> = serde_json::from_str(&moves_json)?;
            moves.truncate(representative_ply.unwrap_or(0) as usize);
            moves
        } else {
            serde_json::from_str(adhoc_history.as_deref().unwrap_or("[]"))?
        };
        let source = match (site, event, relative_path) {
            (Some(site), Some(event), Some(path)) => format!("{site}:{event}:{path}"),
            _ => adhoc_source.unwrap_or_default(),
        };
        candidates.push(CandidateRow {
            candidate_id,
            position_key,
            move_usi,
            history,
            source,
            source_site: SourceSite::from_i32(source_site),
            quality_band,
            priority_key,
            task_id,
            status,
            attempts,
            lease_until,
        });
    }
    Ok(candidates)
}
fn canonical_position_key(sfen: &str) -> String {
    let tokens: Vec<&str> = sfen.split_whitespace().collect();
    if tokens.len() >= 4 {
        tokens[..3].join(" ")
    } else {
        sfen.to_owned()
    }
}

fn position_sfen(position_key: &str) -> String {
    if position_key.split_whitespace().count() == 3 {
        format!("{position_key} 0")
    } else {
        position_key.to_owned()
    }
}
fn unix_time() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
