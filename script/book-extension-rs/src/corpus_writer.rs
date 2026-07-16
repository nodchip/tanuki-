use std::path::{Path, PathBuf};

use rusqlite::{Connection, Transaction, params};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    corpus::{SCHEMA_VERSION, SourceSite},
    record::{NormalizedGame, RecordError},
};

#[derive(Clone, Debug)]
pub struct IngestContext {
    pub site: String,
    pub event: String,
    pub year: i32,
    pub retrieved_at: f64,
    pub priority_key: [u8; 88],
    pub now: f64,
}

#[derive(Debug)]
pub enum RecordOutcome {
    Accepted(NormalizedGame),
    Excluded {
        relative_path: String,
        sha256: String,
        error: RecordError,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IngestBatchResult {
    pub accepted: usize,
    pub excluded: usize,
    pub positions: usize,
    pub candidates: usize,
}

pub struct CorpusWriter {
    path: PathBuf,
    connection: Connection,
}

impl CorpusWriter {
    pub fn create(path: &Path) -> Result<Self, CorpusWriterError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| CorpusWriterError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;",
        )?;
        connection.execute_batch(include_str!("../../corpus_schema.sql"))?;
        create_staging_tables(&connection)?;
        let transaction = connection.unchecked_transaction()?;
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
                "INSERT OR IGNORE INTO meta(key,value) VALUES(?1,?2)",
                params![key, value],
            )?;
        }
        transaction.commit()?;
        Ok(Self {
            path: path.to_path_buf(),
            connection,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }

    pub fn ingest_batch(
        &mut self,
        context: &IngestContext,
        records: Vec<RecordOutcome>,
    ) -> Result<IngestBatchResult, CorpusWriterError> {
        if records.is_empty() {
            return Ok(IngestBatchResult::default());
        }
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(
            "DELETE FROM stage_error;
             DELETE FROM stage_position;
             DELETE FROM stage_game;
             DELETE FROM stage_source;",
        )?;
        let mut result = IngestBatchResult::default();
        for (index, record) in records.into_iter().enumerate() {
            let source_seq = index as i64 + 1;
            match record {
                RecordOutcome::Accepted(game) => {
                    result.accepted += 1;
                    let game_hash = logical_game_hash(&game)?;
                    let moves_json = serde_json::to_string(&game.moves)?;
                    let initial_sfen = game
                        .positions
                        .first()
                        .ok_or(CorpusWriterError::AcceptedGameWithoutPositions)?
                        .sfen
                        .as_str();
                    transaction.execute(
                        "INSERT INTO stage_source(source_seq,relative_path,sha256)
                         VALUES(?1,?2,?3)",
                        params![source_seq, game.source_path, game.sha256],
                    )?;
                    transaction.execute(
                        "INSERT INTO stage_game(
                             source_seq,game_hash,initial_sfen,black_name,white_name,moves_json
                         ) VALUES(?1,?2,?3,?4,?5,?6)",
                        params![
                            source_seq,
                            game_hash,
                            initial_sfen,
                            game.players[0],
                            game.players[1],
                            moves_json,
                        ],
                    )?;
                    for position in &game.positions {
                        transaction.execute(
                            "INSERT INTO stage_position(source_seq,ply,position_key,move)
                             VALUES(?1,?2,?3,?4)",
                            params![
                                source_seq,
                                position.ply as i64,
                                canonical_position_key(&position.sfen),
                                position.move_usi,
                            ],
                        )?;
                    }
                    result.positions += game.positions.len();
                    result.candidates += game.positions.len();
                }
                RecordOutcome::Excluded {
                    relative_path,
                    sha256,
                    error,
                } => {
                    result.excluded += 1;
                    transaction.execute(
                        "INSERT INTO stage_source(source_seq,relative_path,sha256)
                         VALUES(?1,?2,?3)",
                        params![source_seq, relative_path, sha256],
                    )?;
                    transaction.execute(
                        "INSERT INTO stage_error(
                             source_seq,error,error_line,previous_sfen,move,reason
                         ) VALUES(?1,?2,?3,?4,?5,?6)",
                        params![
                            source_seq,
                            error.to_string(),
                            error.line.map(|value| value as i64),
                            error.previous_sfen.as_deref(),
                            error.move_text.as_deref(),
                            format!("{:?}", error.kind),
                        ],
                    )?;
                }
            }
        }
        apply_staged_batch(&transaction, context)?;
        transaction.commit()?;
        Ok(result)
    }
}

fn create_staging_tables(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS stage_source(
             source_seq INTEGER PRIMARY KEY,relative_path TEXT NOT NULL,sha256 TEXT NOT NULL
         );
         CREATE TEMP TABLE IF NOT EXISTS stage_game(
             source_seq INTEGER PRIMARY KEY,game_hash TEXT NOT NULL,initial_sfen TEXT NOT NULL,
             black_name TEXT NOT NULL,white_name TEXT NOT NULL,moves_json TEXT NOT NULL
         );
         CREATE TEMP TABLE IF NOT EXISTS stage_position(
             source_seq INTEGER NOT NULL,ply INTEGER NOT NULL,position_key TEXT NOT NULL,
             move TEXT NOT NULL,PRIMARY KEY(source_seq,ply)
         ) WITHOUT ROWID;
         CREATE TEMP TABLE IF NOT EXISTS stage_error(
             source_seq INTEGER PRIMARY KEY,error TEXT NOT NULL,error_line INTEGER,
             previous_sfen TEXT,move TEXT,reason TEXT NOT NULL
         );",
    )
}

fn apply_staged_batch(
    transaction: &Transaction<'_>,
    context: &IngestContext,
) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "INSERT OR IGNORE INTO raw_source(site,event,year,relative_path,sha256,retrieved_at)
         SELECT ?1,?2,?3,relative_path,sha256,?4 FROM stage_source ORDER BY source_seq",
        params![
            context.site,
            context.event,
            context.year,
            context.retrieved_at
        ],
    )?;
    transaction.execute(
        "INSERT INTO ingest_error(
             source_id,error,error_line,previous_sfen,move,reason,created_at
         )
         SELECT DISTINCT rs.id,se.error,se.error_line,se.previous_sfen,se.move,se.reason,?1
         FROM stage_error se
         JOIN stage_source ss ON ss.source_seq=se.source_seq
         JOIN raw_source rs
           ON rs.site=?2 AND rs.relative_path=ss.relative_path AND rs.sha256=ss.sha256
         WHERE NOT EXISTS(
             SELECT 1 FROM ingest_error old
             WHERE old.source_id=rs.id AND old.reason=se.reason
               AND old.error_line IS se.error_line
               AND old.previous_sfen IS se.previous_sfen AND old.move IS se.move
         ) ORDER BY se.source_seq",
        params![context.now, context.site],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO logical_game(
             game_hash,initial_sfen,black_name,white_name,moves_json,primary_source_id
         )
         SELECT sg.game_hash,sg.initial_sfen,sg.black_name,sg.white_name,sg.moves_json,rs.id
         FROM stage_game sg
         JOIN stage_source ss ON ss.source_seq=sg.source_seq
         JOIN raw_source rs
           ON rs.site=?1 AND rs.relative_path=ss.relative_path AND rs.sha256=ss.sha256
         ORDER BY sg.source_seq",
        params![context.site],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO source_game(source_id,game_id)
         SELECT rs.id,lg.id FROM stage_game sg
         JOIN stage_source ss ON ss.source_seq=sg.source_seq
         JOIN raw_source rs
           ON rs.site=?1 AND rs.relative_path=ss.relative_path AND rs.sha256=ss.sha256
         JOIN logical_game lg ON lg.game_hash=sg.game_hash ORDER BY sg.source_seq",
        params![context.site],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO position(position_key)
         SELECT position_key FROM stage_position GROUP BY position_key
         ORDER BY MIN(source_seq),MIN(ply)",
        [],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO game_position(game_id,ply,position_id,move)
         SELECT lg.id,sp.ply,p.id,sp.move FROM stage_position sp
         JOIN stage_game sg ON sg.source_seq=sp.source_seq
         JOIN logical_game lg ON lg.game_hash=sg.game_hash
         JOIN position p ON p.position_key=sp.position_key
         ORDER BY sp.source_seq,sp.ply",
        [],
    )?;
    transaction.execute(
        "WITH ranked AS (
             SELECT gp.position_id,gp.move,lg.id AS game_id,gp.ply,rs.id AS source_id,
                    sg.source_seq,
                    ROW_NUMBER() OVER(
                        PARTITION BY gp.position_id,gp.move ORDER BY sg.source_seq,gp.ply
                    ) AS candidate_rank
             FROM stage_game sg
             JOIN stage_source ss ON ss.source_seq=sg.source_seq
             JOIN raw_source rs
               ON rs.site=?1 AND rs.relative_path=ss.relative_path AND rs.sha256=ss.sha256
             JOIN logical_game lg ON lg.game_hash=sg.game_hash
             JOIN game_position gp ON gp.game_id=lg.id
         )
         INSERT INTO candidate(
             position_id,move,representative_game_id,representative_ply,source_id,
             priority_key,quality_band,source_site,recent_occurrences,occurrences,
             active,created_at,updated_at
         )
         SELECT position_id,move,game_id,ply,source_id,?2,4,?3,0,0,1,?4,?4
         FROM ranked WHERE candidate_rank=1 ORDER BY source_seq,ply
         ON CONFLICT(position_id,move) DO UPDATE SET
           representative_game_id=CASE WHEN excluded.priority_key>candidate.priority_key
             THEN excluded.representative_game_id ELSE candidate.representative_game_id END,
           representative_ply=CASE WHEN excluded.priority_key>candidate.priority_key
             THEN excluded.representative_ply ELSE candidate.representative_ply END,
           source_id=CASE WHEN excluded.priority_key>candidate.priority_key
             THEN excluded.source_id ELSE candidate.source_id END,
           priority_key=MAX(candidate.priority_key,excluded.priority_key),
           active=1,updated_at=excluded.updated_at",
        params![
            context.site,
            context.priority_key.as_slice(),
            SourceSite::from_name(&context.site) as i32,
            context.now
        ],
    )?;
    Ok(())
}

fn logical_game_hash(game: &NormalizedGame) -> Result<String, serde_json::Error> {
    let moves = serde_json::to_string(&game.moves)?;
    let players = serde_json::to_string(&game.players)?;
    let payload = format!("{{\"moves\":{moves},\"players\":{players}}}");
    Ok(format!("{:x}", Sha256::digest(payload.as_bytes())))
}

fn canonical_position_key(sfen: &str) -> String {
    sfen.split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Error)]
pub enum CorpusWriterError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("filesystem error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("accepted game has no positions")]
    AcceptedGameWithoutPositions,
}
