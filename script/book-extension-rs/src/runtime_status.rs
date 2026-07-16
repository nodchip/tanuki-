use std::{collections::BTreeMap, io::Write, path::Path};

use serde::Serialize;
use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Clone, Debug, Default, Serialize)]
pub struct TaskStatusCounts {
    pub pending: i64,
    pub running: i64,
    pub evaluated: i64,
    pub permanent_failed: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LastCandidateStatus {
    pub candidate_id: i64,
    pub position_key: String,
    pub move_usi: String,
    pub source: String,
    pub quality_band: i32,
    pub result: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BookSaveStatus {
    pub saved_at: f64,
    pub path: String,
    pub success: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeStatusSnapshot {
    pub updated_at: f64,
    pub active_quality_band: i32,
    pub progressive_width: i32,
    pub eligible_miss_count: i64,
    pub tasks: TaskStatusCounts,
    pub book_add_successes: i64,
    pub site_nodes: BTreeMap<String, i64>,
    pub last_candidate: Option<LastCandidateStatus>,
    pub corpus_revision: i64,
    pub priority_policy_version: String,
    pub engine_fingerprint: String,
    pub last_book_save: Option<BookSaveStatus>,
}

#[derive(Debug, Error)]
pub enum RuntimeStatusError {
    #[error("status I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("status JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("status atomic persist failed: {0}")]
    Persist(#[from] tempfile::PersistError),
}

pub fn write_runtime_status_atomic(
    path: &Path,
    snapshot: &RuntimeStatusSnapshot,
) -> Result<(), RuntimeStatusError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, snapshot)?;
    temporary.write_all(b"\n")?;
    temporary.as_file_mut().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}
