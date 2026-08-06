use std::{
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use tempfile::NamedTempFile;
use thiserror::Error;

use crate::{
    book::{
        BookEntry, BookParseError, BookPosition, DEFAULT_HEADER, parse_book_entry_line,
        replace_sfen_ply,
    },
    depth_histogram::{self, DepthHistogramReport, DepthHistogramSeries, record_in_transaction},
    search::{LeafPath, SearchBook, SearchError, SearchResult},
    storage::{rotate_backups, sha256_file},
    validation::{IssueKind, ValidationFileError, ValidationReport, validate_file},
};

const SCHEMA_VERSION: i64 = 1;
const IMPORT_BATCH_POSITIONS: usize = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportReport {
    pub input_sha256: String,
    pub already_imported: bool,
    pub inserted_positions: u64,
    pub inserted_moves: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplySearchOutcome {
    pub new_position: bool,
    pub inserted_moves: u64,
    pub updated_moves: u64,
}

#[derive(Debug, Error)]
pub enum SqliteBookError {
    #[error("SQLite book I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite book operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("book parse failed: {0}")]
    Parse(#[from] BookParseError),
    #[error("book validation failed: {0}")]
    ValidationFile(#[from] ValidationFileError),
    #[error("book validation found {0} issue(s)")]
    InvalidBook(usize),
    #[error("book SFEN groups are not strictly sorted")]
    UnsortedBook,
    #[error("SQLite book uses ignore_ply={actual}, but the requested value is {requested}")]
    IgnorePlyMismatch { actual: bool, requested: bool },
    #[error("SQLite book schema version {actual} is not supported (expected {expected})")]
    SchemaVersion { actual: i64, expected: i64 },
    #[error("existing SQLite opening book does not exist: {}", .0.display())]
    MissingDatabase(PathBuf),
    #[error("existing SQLite opening book is not initialized: {}", .0.display())]
    UninitializedDatabase(PathBuf),
    #[error("existing SQLite opening book has no positions: {}", .0.display())]
    EmptyDatabase(PathBuf),
    #[error("path position is missing: {0}")]
    MissingPosition(String),
    #[error("path move is missing: {sfen} {move_usi}")]
    MissingMove { sfen: String, move_usi: String },
    #[error("atomic persist failed: {0}")]
    Persist(#[from] tempfile::PersistError),
}

pub struct SqliteOpeningBook {
    path: PathBuf,
    connection: Connection,
    ignore_ply: bool,
}

impl SqliteOpeningBook {
    pub fn open(path: &Path, ignore_ply: bool) -> Result<Self, SqliteBookError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        configure_connection(&connection)?;
        create_schema(&connection, ignore_ply)?;
        validate_metadata(&connection, ignore_ply)?;
        Ok(Self {
            path: path.to_path_buf(),
            connection,
            ignore_ply,
        })
    }

    pub fn open_existing(path: &Path, ignore_ply: bool) -> Result<Self, SqliteBookError> {
        if !path.is_file() {
            return Err(SqliteBookError::MissingDatabase(path.to_path_buf()));
        }
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.busy_timeout(Duration::from_secs(30))?;
        let required_tables: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table'
               AND name IN ('book_meta', 'import_history', 'position', 'move')",
            [],
            |row| row.get(0),
        )?;
        if required_tables != 4 {
            return Err(SqliteBookError::UninitializedDatabase(path.to_path_buf()));
        }
        validate_metadata(&connection, ignore_ply)?;
        let has_positions: bool =
            connection.query_row("SELECT EXISTS(SELECT 1 FROM position LIMIT 1)", [], |row| {
                row.get(0)
            })?;
        if !has_positions {
            return Err(SqliteBookError::EmptyDatabase(path.to_path_buf()));
        }
        configure_connection(&connection)?;
        create_schema(&connection, ignore_ply)?;
        Ok(Self {
            path: path.to_path_buf(),
            connection,
            ignore_ply,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn ignore_ply(&self) -> bool {
        self.ignore_ply
    }

    pub fn position_key(&self, sfen: &str) -> String {
        canonical_sfen(sfen, self.ignore_ply)
    }

    pub fn output_sfen(&self, sfen: &str) -> String {
        canonical_sfen(sfen, self.ignore_ply)
    }

    pub fn positions_len(&self) -> Result<u64, SqliteBookError> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM position", [], |row| row.get(0))?)
    }

    pub fn position(&self, sfen: &str) -> Result<Option<BookPosition>, SqliteBookError> {
        let key = self.position_key(sfen);
        let position_id: Option<i64> = self
            .connection
            .query_row(
                "SELECT position_id FROM position WHERE sfen = ?1",
                [&key],
                |row| row.get(0),
            )
            .optional()?;
        let Some(position_id) = position_id else {
            return Ok(None);
        };
        let mut statement = self.connection.prepare(
            "SELECT move_usi, response_usi, eval_cp, depth, visits, order_index
             FROM move WHERE position_id = ?1
             ORDER BY eval_cp DESC, order_index ASC",
        )?;
        let entries = statement
            .query_map([position_id], |row| {
                Ok(BookEntry::new(
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get::<_, i64>(5)? as usize,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(BookPosition {
            sfen: self.output_sfen(&key),
            entries,
            order_index: 0,
        }))
    }

    pub fn import_yaneuraou(&mut self, path: &Path) -> Result<ImportReport, SqliteBookError> {
        self.import_yaneuraou_with_sort_requirement(path, true)
    }

    pub fn import_yaneuraou_compatible(
        &mut self,
        path: &Path,
    ) -> Result<ImportReport, SqliteBookError> {
        self.import_yaneuraou_with_sort_requirement(path, false)
    }

    fn import_yaneuraou_with_sort_requirement(
        &mut self,
        path: &Path,
        require_sorted: bool,
    ) -> Result<ImportReport, SqliteBookError> {
        let validation = validate_file(path)?;
        let has_unsorted = validation
            .issues
            .iter()
            .any(|issue| issue.kind == IssueKind::NonIncreasingSfen);
        let other_issues = validation
            .issues
            .iter()
            .filter(|issue| issue.kind != IssueKind::NonIncreasingSfen)
            .count();
        if require_sorted && has_unsorted {
            return Err(SqliteBookError::UnsortedBook);
        }
        if other_issues != 0 {
            return Err(SqliteBookError::InvalidBook(validation.issues.len()));
        }
        let input_sha256 = sha256_file(path)?;
        let already_imported = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM import_history WHERE input_sha256 = ?1)",
            [&input_sha256],
            |row| row.get::<_, bool>(0),
        )?;
        if already_imported {
            return Ok(ImportReport {
                input_sha256,
                already_imported: true,
                inserted_positions: 0,
                inserted_moves: 0,
            });
        }

        let initially_empty = self.positions_len()? == 0;
        let file = File::open(path)?;
        let reader = BufReader::with_capacity(1024 * 1024, file);
        let mut header = None;
        let mut current_sfen = None;
        let mut current_entries = Vec::new();
        let mut batch = Vec::with_capacity(IMPORT_BATCH_POSITIONS);
        let mut order_index = 0;
        let mut inserted_positions = 0;
        let mut inserted_moves = 0;

        for (line_index, line) in reader.lines().enumerate() {
            let line = line?;
            let line = if line_index == 0 {
                line.trim_start_matches('\u{feff}').trim()
            } else {
                line.trim()
            };
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if line.starts_with('#') {
                if line.starts_with("#YANEURAOU-DB") {
                    header = Some(line.to_owned());
                }
                continue;
            }
            if let Some(sfen) = line.strip_prefix("sfen ") {
                if let Some(previous) = current_sfen.take() {
                    batch.push((previous, std::mem::take(&mut current_entries)));
                }
                if batch.len() >= IMPORT_BATCH_POSITIONS {
                    let counts = self.import_batch(&batch)?;
                    inserted_positions += counts.0;
                    inserted_moves += counts.1;
                    batch.clear();
                }
                current_sfen = Some(self.output_sfen(sfen.trim()));
                continue;
            }
            current_entries.push(parse_book_entry_line(line, order_index)?);
            order_index += 1;
        }
        if let Some(previous) = current_sfen {
            batch.push((previous, current_entries));
        }
        if !batch.is_empty() {
            let counts = self.import_batch(&batch)?;
            inserted_positions += counts.0;
            inserted_moves += counts.1;
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if initially_empty && let Some(header) = header {
            transaction.execute(
                "UPDATE book_meta SET yaneuraou_header = ?1 WHERE singleton = 1",
                [header],
            )?;
        }
        transaction.execute(
            "INSERT INTO import_history(input_sha256, imported_at_unix) VALUES (?1, unixepoch())",
            [&input_sha256],
        )?;
        transaction.execute(
            "UPDATE book_meta SET book_revision = book_revision + 1 WHERE singleton = 1",
            [],
        )?;
        transaction.commit()?;
        Ok(ImportReport {
            input_sha256,
            already_imported: false,
            inserted_positions,
            inserted_moves,
        })
    }

    fn import_batch(
        &mut self,
        batch: &[(String, Vec<BookEntry>)],
    ) -> Result<(u64, u64), SqliteBookError> {
        let ignore_ply = self.ignore_ply;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut inserted_positions = 0;
        let mut inserted_moves = 0;
        {
            let mut insert_position =
                transaction.prepare("INSERT OR IGNORE INTO position(sfen) VALUES (?1)")?;
            let mut find_position =
                transaction.prepare("SELECT position_id FROM position WHERE sfen = ?1")?;
            let mut find_next_order = transaction.prepare(
                "SELECT COALESCE(MAX(order_index) + 1, 0) FROM move WHERE position_id = ?1",
            )?;
            let mut insert_move = transaction.prepare(
                "INSERT OR IGNORE INTO move(
                         position_id, move_usi, response_usi, eval_cp, depth, visits, order_index
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (sfen, entries) in batch {
                let key = canonical_sfen(sfen, ignore_ply);
                inserted_positions += insert_position.execute([&key])? as u64;
                let position_id: i64 = find_position.query_row([&key], |row| row.get(0))?;
                let mut next_order: i64 =
                    find_next_order.query_row([position_id], |row| row.get(0))?;
                for entry in entries {
                    let changed = insert_move.execute(params![
                        position_id,
                        entry.move_usi,
                        entry.response,
                        entry.eval_cp,
                        entry.depth,
                        entry.visits,
                        next_order,
                    ])?;
                    if changed != 0 {
                        inserted_moves += 1;
                        next_order += 1;
                    }
                }
            }
        }
        transaction.commit()?;
        Ok((inserted_positions, inserted_moves))
    }

    pub fn add_result_if_absent(
        &mut self,
        sfen: &str,
        result: &SearchResult,
    ) -> Result<bool, SqliteBookError> {
        let key = self.position_key(sfen);
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let position_id = ensure_position(&transaction, &key)?;
        let next_order: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(order_index) + 1, 0) FROM move WHERE position_id = ?1",
            [position_id],
            |row| row.get(0),
        )?;
        let added = transaction.execute(
            "INSERT OR IGNORE INTO move(
                 position_id, move_usi, response_usi, eval_cp, depth, visits, order_index
             ) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            params![
                position_id,
                result.move_usi,
                result.response,
                result.eval_cp,
                result.depth,
                next_order,
            ],
        )? != 0;
        if added {
            bump_revision(&transaction)?;
        }
        transaction.commit()?;
        Ok(added)
    }

    pub fn apply_search_results(
        &mut self,
        path: &LeafPath,
        results: &[SearchResult],
    ) -> Result<bool, SqliteBookError> {
        Ok(self
            .apply_search_results_internal(path, results, None)?
            .new_position)
    }

    pub fn apply_search_results_tracked(
        &mut self,
        path: &LeafPath,
        results: &[SearchResult],
        lane: &str,
        depth: usize,
    ) -> Result<ApplySearchOutcome, SqliteBookError> {
        self.apply_search_results_internal(path, results, Some((lane, depth)))
    }

    fn apply_search_results_internal(
        &mut self,
        path: &LeafPath,
        results: &[SearchResult],
        histogram: Option<(&str, usize)>,
    ) -> Result<ApplySearchOutcome, SqliteBookError> {
        let leaf_key = self.position_key(&path.leaf_sfen);
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existed = position_id(&transaction, &leaf_key)?.is_some();
        let leaf_id = ensure_position(&transaction, &leaf_key)?;
        let mut next_order: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(order_index) + 1, 0) FROM move WHERE position_id = ?1",
            [leaf_id],
            |row| row.get(0),
        )?;
        let mut inserted_moves = 0_u64;
        let mut updated_moves = 0_u64;
        for result in results {
            let updated = transaction.execute(
                "UPDATE move
                 SET response_usi = CASE
                         WHEN lower(?3) = 'none' AND lower(response_usi) <> 'none'
                         THEN response_usi ELSE ?3 END,
                     eval_cp = ?4,
                     depth = ?5
                 WHERE position_id = ?1 AND move_usi = ?2",
                params![
                    leaf_id,
                    result.move_usi,
                    result.response,
                    result.eval_cp,
                    result.depth,
                ],
            )?;
            if updated == 0 {
                transaction.execute(
                    "INSERT INTO move(
                         position_id, move_usi, response_usi, eval_cp, depth, visits, order_index
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
                    params![
                        leaf_id,
                        result.move_usi,
                        result.response,
                        result.eval_cp,
                        result.depth,
                        next_order,
                    ],
                )?;
                next_order += 1;
                inserted_moves += 1;
            } else {
                updated_moves += 1;
            }
        }
        for step in &path.steps {
            let key = canonical_sfen(&step.sfen, self.ignore_ply);
            let Some(id) = position_id(&transaction, &key)? else {
                return Err(SqliteBookError::MissingPosition(step.sfen.clone()));
            };
            if transaction.execute(
                "UPDATE move SET visits = visits + 1
                 WHERE position_id = ?1 AND move_usi = ?2",
                params![id, step.move_usi],
            )? == 0
            {
                return Err(SqliteBookError::MissingMove {
                    sfen: step.sfen.clone(),
                    move_usi: step.move_usi.clone(),
                });
            }
        }
        let mut current_value = node_value(&transaction, leaf_id)?;
        for step in path.steps.iter().rev() {
            let key = canonical_sfen(&step.sfen, self.ignore_ply);
            let Some(id) = position_id(&transaction, &key)? else {
                return Err(SqliteBookError::MissingPosition(step.sfen.clone()));
            };
            if transaction.execute(
                "UPDATE move SET eval_cp = ?3 WHERE position_id = ?1 AND move_usi = ?2",
                params![id, step.move_usi, -current_value],
            )? == 0
            {
                return Err(SqliteBookError::MissingMove {
                    sfen: step.sfen.clone(),
                    move_usi: step.move_usi.clone(),
                });
            }
            current_value = node_value(&transaction, id)?;
        }
        bump_revision(&transaction)?;
        if let Some((lane, depth)) = histogram {
            record_in_transaction(&transaction, lane, "search", depth)?;
            let event_kind = if !existed {
                "new-position"
            } else if inserted_moves > 0 {
                "new-move"
            } else {
                "reevaluated"
            };
            record_in_transaction(&transaction, lane, event_kind, depth)?;
        }
        transaction.commit()?;
        Ok(ApplySearchOutcome {
            new_position: !existed,
            inserted_moves,
            updated_moves,
        })
    }

    pub fn record_depth_search(&mut self, lane: &str, depth: usize) -> Result<(), SqliteBookError> {
        depth_histogram::record(&mut self.connection, lane, "search", depth)?;
        Ok(())
    }

    pub fn flush_depth_histogram(
        &mut self,
        run_id: &str,
        now: f64,
    ) -> Result<Option<DepthHistogramReport>, SqliteBookError> {
        Ok(depth_histogram::flush(&mut self.connection, run_id, now)?)
    }

    pub fn depth_histogram_totals(&self) -> Result<Vec<DepthHistogramSeries>, SqliteBookError> {
        Ok(depth_histogram::totals(&self.connection)?)
    }

    pub fn move_usis(&self, sfen: &str) -> Result<Vec<String>, SqliteBookError> {
        let Some(position) = self.position(sfen)? else {
            return Ok(Vec::new());
        };
        Ok(position
            .entries
            .into_iter()
            .map(|entry| entry.move_usi)
            .collect())
    }
}

impl SearchBook for SqliteOpeningBook {
    fn search_position(&self, sfen: &str) -> Result<Option<BookPosition>, SearchError> {
        self.position(sfen)
            .map_err(|error| SearchError::Book(error.to_string()))
    }

    fn search_position_key(&self, sfen: &str) -> String {
        self.position_key(sfen)
    }
}

pub fn export_yaneuraou_atomic(
    database_path: &Path,
    output_path: &Path,
    backup_count: usize,
) -> Result<ValidationReport, SqliteBookError> {
    let mut connection =
        Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(30))?;
    let transaction = connection.transaction()?;
    let header: String = transaction.query_row(
        "SELECT yaneuraou_header FROM book_meta WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    let parent = output_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    {
        let mut writer = BufWriter::with_capacity(1024 * 1024, temporary.as_file_mut());
        writeln!(writer, "{header}")?;
        let mut statement = transaction.prepare(
            "SELECT p.sfen, m.move_usi, m.response_usi, m.eval_cp, m.depth, m.visits
             FROM position p
             LEFT JOIN move m ON m.position_id = p.position_id
             ORDER BY p.sfen ASC, m.eval_cp DESC, m.order_index ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut previous_sfen = String::new();
        while let Some(row) = rows.next()? {
            let sfen: String = row.get(0)?;
            if sfen != previous_sfen {
                writeln!(writer, "sfen {sfen}")?;
                previous_sfen = sfen;
            }
            let move_usi: Option<String> = row.get(1)?;
            if let Some(move_usi) = move_usi {
                writeln!(
                    writer,
                    "{} {} {} {} {}",
                    move_usi,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, i32>(4)?,
                    row.get::<_, u64>(5)?,
                )?;
            }
        }
        writer.flush()?;
    }
    temporary.as_file_mut().sync_all()?;
    drop(transaction);
    let report = validate_file(temporary.path())?;
    if !report.valid() {
        return Err(SqliteBookError::InvalidBook(report.issues.len()));
    }
    rotate_backups(output_path, backup_count)?;
    temporary.persist(output_path)?;
    Ok(report)
}

fn configure_connection(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.busy_timeout(Duration::from_secs(30))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

fn canonical_sfen(sfen: &str, ignore_ply: bool) -> String {
    if ignore_ply {
        replace_sfen_ply(sfen, "0")
    } else {
        sfen.to_owned()
    }
}

fn validate_metadata(connection: &Connection, ignore_ply: bool) -> Result<(), SqliteBookError> {
    let (schema_version, stored_ignore_ply): (i64, i64) = connection.query_row(
        "SELECT schema_version, ignore_ply FROM book_meta WHERE singleton = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if schema_version != SCHEMA_VERSION {
        return Err(SqliteBookError::SchemaVersion {
            actual: schema_version,
            expected: SCHEMA_VERSION,
        });
    }
    if stored_ignore_ply != i64::from(ignore_ply) {
        return Err(SqliteBookError::IgnorePlyMismatch {
            actual: stored_ignore_ply != 0,
            requested: ignore_ply,
        });
    }
    Ok(())
}

fn create_schema(connection: &Connection, ignore_ply: bool) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS book_meta (
             singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
             schema_version INTEGER NOT NULL,
             yaneuraou_header TEXT NOT NULL,
             ignore_ply INTEGER NOT NULL CHECK(ignore_ply IN (0, 1)),
             book_revision INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS import_history (
             import_id INTEGER PRIMARY KEY,
             input_sha256 TEXT NOT NULL UNIQUE,
             imported_at_unix INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS position (
             position_id INTEGER PRIMARY KEY,
             sfen TEXT NOT NULL UNIQUE
         );
         CREATE TABLE IF NOT EXISTS move (
             position_id INTEGER NOT NULL REFERENCES position(position_id) ON DELETE CASCADE,
             move_usi TEXT NOT NULL,
             response_usi TEXT NOT NULL,
             eval_cp INTEGER NOT NULL,
             depth INTEGER NOT NULL,
             visits INTEGER NOT NULL CHECK(visits >= 0),
             order_index INTEGER NOT NULL,
             PRIMARY KEY(position_id, move_usi)
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS move_export_order
         ON move(position_id, eval_cp DESC, order_index ASC);",
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO book_meta(
             singleton, schema_version, yaneuraou_header, ignore_ply, book_revision
         ) VALUES (1, ?1, ?2, ?3, 0)",
        params![SCHEMA_VERSION, DEFAULT_HEADER, i64::from(ignore_ply)],
    )?;
    depth_histogram::create_schema(connection)?;
    Ok(())
}

fn ensure_position(transaction: &Transaction<'_>, sfen: &str) -> Result<i64, rusqlite::Error> {
    transaction.execute("INSERT OR IGNORE INTO position(sfen) VALUES (?1)", [sfen])?;
    transaction.query_row(
        "SELECT position_id FROM position WHERE sfen = ?1",
        [sfen],
        |row| row.get(0),
    )
}

fn position_id(transaction: &Transaction<'_>, sfen: &str) -> Result<Option<i64>, rusqlite::Error> {
    transaction
        .query_row(
            "SELECT position_id FROM position WHERE sfen = ?1",
            [sfen],
            |row| row.get(0),
        )
        .optional()
}

fn node_value(transaction: &Transaction<'_>, position_id: i64) -> Result<i32, rusqlite::Error> {
    transaction.query_row(
        "SELECT COALESCE(MAX(eval_cp), 0) FROM move WHERE position_id = ?1",
        [position_id],
        |row| row.get(0),
    )
}

fn bump_revision(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "UPDATE book_meta SET book_revision = book_revision + 1 WHERE singleton = 1",
        [],
    )?;
    Ok(())
}
