use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shogi_legality_lite::is_legal_partial;
use thiserror::Error;

use crate::{
    config::ExtensionConfig,
    corpus_build_profile::{ArchiveKind, CorpusBuildProfile, CorpusManifest},
    corpus_coverage::{CoverageReport, generate_coverage_report},
    corpus_download::{DownloadError, DownloadSummary, collect_manifest_with_control},
    corpus_metadata::{
        FloodgateRating, ParticipantAliasDocument, RatingPolicy, TournamentRanking,
        import_floodgate_rating, import_participant_aliases, import_tournament_ranking,
        recompute_candidate_priorities_with_control, unmatched_participants,
    },
    corpus_source::{
        ArchiveLimits, RecordFormat, SevenZipInfo, SourceVisitStats, VisitRecordsError,
        inspect_seven_zip, visit_records,
    },
    corpus_state_migration::{
        MigrationIdentity, MigrationSummary, migrate_runtime_state, persist_migration_identity,
    },
    corpus_writer::{
        CorpusWriter, CorpusWriterError, IngestBatchResult, IngestContext, RecordOutcome,
    },
    csa::{csa_source_sha256, parse_csa},
    kif::parse_kif,
    priority::{POLICY_VERSION, PriorityFacts, encode_priority, priority_tuple},
    record::RecordError,
    runtime::BookFileLock,
    storage::sha256_file,
    validation::{parse_move, parse_position},
};

const INGEST_BATCH_SIZE: usize = 500;

#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub profile_path: PathBuf,
    pub state_dir: PathBuf,
    pub input_book: Option<PathBuf>,
    pub seven_zip: Option<PathBuf>,
    pub stop: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BuildCheckpoint {
    version: u32,
    profile_sha256: String,
    manifest_sha256: String,
    completed_records: BTreeMap<String, u64>,
}

impl BuildCheckpoint {
    fn new(profile_sha256: String, manifest_sha256: String) -> Self {
        Self {
            version: 1,
            profile_sha256,
            manifest_sha256,
            completed_records: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct BuildSummary {
    pub status: String,
    pub started_at: u64,
    pub finished_at: u64,
    pub elapsed_seconds: f64,
    pub source_count: usize,
    pub record_count: u64,
    pub record_members: u64,
    pub filtered_members: u64,
    pub records_per_second: f64,
    pub moves_per_second: f64,
    pub positions_per_second: f64,
    pub sqlite_rows_per_second: f64,
    pub profile: String,
    pub run_id: String,
    pub resumed: bool,
    pub resumed_records: u64,
    pub workspace: String,
    pub accepted: usize,
    pub excluded: usize,
    pub positions: usize,
    pub candidates: usize,
    pub ranking_snapshots: usize,
    pub rating_snapshots: usize,
    pub unmatched_participants: BTreeMap<String, i64>,
    pub runtime_state_migration: Option<MigrationSummary>,
    pub database_size: u64,
    pub database_sha256: String,
    pub database_wal_size: u64,
    pub snapshot_sha256: String,
    pub coverage_sha256: String,
    pub input_book_sha256: String,
    pub profile_sha256: String,
    pub priority_policy_version: &'static str,
    pub manifest_sha256: String,
    pub download: DownloadSummary,
    pub database_checks: DatabaseChecks,
    pub coverage: CoverageReport,
    pub phase_seconds: BTreeMap<String, f64>,
    pub storage: StorageEstimate,
    pub seven_zip: Option<SevenZipInfo>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DatabaseChecks {
    pub integrity_check: String,
    pub foreign_key_check: String,
    pub schema_version: i64,
    pub raw_sources: i64,
    pub logical_games: i64,
    pub ingest_errors: i64,
    pub candidates: i64,
    pub candidate_sources: i64,
    pub positions: i64,
    pub game_positions: i64,
    pub sqlite_rows: i64,
    pub semantic_games: i64,
    pub semantic_positions: i64,
    pub ingest_error_reasons: BTreeMap<String, i64>,
    pub candidate_query_plan: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StorageEstimate {
    pub archive_bytes: u64,
    pub missing_download_bytes: u64,
    pub estimated_database_bytes: u64,
    pub safety_bytes: u64,
    pub required_free_bytes: u64,
    pub free_bytes_before: u64,
}

pub fn build_corpus(options: &BuildOptions) -> Result<BuildSummary, BuildError> {
    std::fs::create_dir_all(&options.state_dir).map_err(|source| BuildError::Io {
        path: options.state_dir.clone(),
        source,
    })?;
    let started_at = unix_seconds();
    let run_id = format!("{}-{}", started_at, std::process::id());
    let run_dir = options.state_dir.join("build").join(&run_id);
    std::fs::create_dir_all(&run_dir).map_err(|source| BuildError::Io {
        path: run_dir.clone(),
        source,
    })?;
    let profile = match CorpusBuildProfile::load(&options.profile_path) {
        Ok(profile) => profile,
        Err(error) => {
            let error = BuildError::Profile(error);
            write_failure_summary(
                &run_dir,
                &options.profile_path.to_string_lossy(),
                &run_id,
                started_at,
                &error,
            );
            return Err(error);
        }
    };
    let result = clear_stale_stop_request(&options.state_dir)
        .and_then(|()| build_corpus_run(options, &profile, &run_id, &run_dir, started_at));
    if let Err(error) = &result {
        write_failure_summary(&run_dir, &profile.name, &run_id, started_at, error);
    }
    result
}

fn build_corpus_run(
    options: &BuildOptions,
    profile: &CorpusBuildProfile,
    run_id: &str,
    run_dir: &Path,
    started_at: u64,
) -> Result<BuildSummary, BuildError> {
    let manifest = CorpusManifest::load(&profile.manifest)?;
    let requires_seven_zip = manifest.sources.iter().any(|source| {
        matches!(
            ArchiveKind::from_relative_path(&source.relative_path),
            Ok(ArchiveKind::SevenZip | ArchiveKind::Lzh)
        )
    });
    let seven_zip = if requires_seven_zip {
        Some(inspect_seven_zip(options.seven_zip.as_deref())?)
    } else {
        None
    };
    let lock_metadata = format!(r#"{{"pid":{}}}"#, std::process::id());
    let _build_lock =
        BookFileLock::acquire(&options.state_dir.join("corpus-build.lock"), &lock_metadata)?;
    let _runtime_lock = BookFileLock::acquire(
        &options.state_dir.join("book-extension.lock"),
        &lock_metadata,
    )?;
    let download_dir = options.state_dir.join("downloads").join(&profile.name);
    let profile_sha256 = sha256_file(&options.profile_path).map_err(|source| BuildError::Io {
        path: options.profile_path.clone(),
        source,
    })?;
    let manifest_sha256 = sha256_file(&profile.manifest).map_err(|source| BuildError::Io {
        path: profile.manifest.clone(),
        source,
    })?;
    let workspace_key = format!(
        "{:x}",
        Sha256::digest(format!("{profile_sha256}\0{manifest_sha256}").as_bytes())
    );
    let workspace = options.state_dir.join("work").join(&workspace_key[..24]);
    std::fs::create_dir_all(&workspace).map_err(|source| BuildError::Io {
        path: workspace.clone(),
        source,
    })?;
    let database = workspace.join("corpus.sqlite");
    let checkpoint_path = workspace.join("checkpoint.json");
    let (mut checkpoint, resumed) = load_checkpoint(
        &checkpoint_path,
        &database,
        &profile_sha256,
        &manifest_sha256,
        run_id,
    )?;
    let mut phase_seconds = BTreeMap::new();

    phase_start("storage");
    let started = Instant::now();
    let mut storage = storage_estimate(&manifest, &download_dir);
    storage.free_bytes_before = available_space(&options.state_dir)?;
    if storage.free_bytes_before < storage.required_free_bytes {
        return Err(BuildError::InsufficientSpace {
            required: storage.required_free_bytes,
            available: storage.free_bytes_before,
        });
    }
    phase_seconds.insert("storage".to_owned(), started.elapsed().as_secs_f64());

    check_stop(options, "storage")?;
    phase_start("download");
    let started = Instant::now();
    let download =
        match collect_manifest_with_control(&manifest, &download_dir, || stop_requested(options)) {
            Ok(summary) => summary,
            Err(DownloadError::Stopped) => {
                return Err(BuildError::Stopped {
                    phase: "download".to_owned(),
                });
            }
            Err(error) => return Err(error.into()),
        };
    phase_seconds.insert("download".to_owned(), started.elapsed().as_secs_f64());

    check_stop(options, "download")?;
    phase_start("ingest");
    let started = Instant::now();
    let mut writer = CorpusWriter::create(&database)?;
    write_checkpoint(&checkpoint_path, &checkpoint)?;
    let (ingest, resumed_records, source_stats) = ingest_profile(
        &mut writer,
        profile,
        &download_dir,
        seven_zip.as_ref().map(|info| info.path.as_path()),
        options,
        &checkpoint_path,
        &mut checkpoint,
    )?;
    if ingest.accepted > 0 {
        writer.connection_mut().execute(
            "UPDATE meta SET value='1'
             WHERE key='corpus_revision' AND CAST(value AS INTEGER)<1",
            [],
        )?;
    }
    phase_seconds.insert("ingest".to_owned(), started.elapsed().as_secs_f64());

    check_stop(options, "ingest")?;
    phase_start("metadata");
    let started = Instant::now();
    let ranking_snapshots = import_rankings(writer.connection_mut(), &profile.ranking_files)?;
    import_aliases(writer.connection_mut(), &profile.alias_files)?;
    let rating_snapshots = import_rating(writer.connection_mut(), profile)?;
    match recompute_candidate_priorities_with_control(
        writer.connection_mut(),
        profile.priority_reference_year,
        || stop_requested(options),
    ) {
        Ok(_) => {}
        Err(crate::corpus_metadata::MetadataError::Stopped) => {
            return Err(BuildError::Stopped {
                phase: "metadata".to_owned(),
            });
        }
        Err(error) => return Err(error.into()),
    }
    let unmatched_participants = unmatched_participants(writer.connection())?;
    phase_seconds.insert("metadata".to_owned(), started.elapsed().as_secs_f64());

    check_stop(options, "metadata")?;
    phase_start("coverage");
    let started = Instant::now();
    let input_book = options
        .input_book
        .as_ref()
        .or(profile.input_book.as_ref())
        .cloned()
        .unwrap_or_else(|| options.state_dir.join("input-book.db"));
    let book_hash = sha256_file(&input_book).map_err(|source| BuildError::Io {
        path: input_book.clone(),
        source,
    })?;
    let coverage = generate_coverage_report(
        writer.connection_mut(),
        &input_book,
        &profile.coverage_snapshot_id,
        &book_hash,
    )?;
    write_json(&run_dir.join("coverage-initial.json"), &coverage)?;
    phase_seconds.insert("coverage".to_owned(), started.elapsed().as_secs_f64());
    let migration_identity = MigrationIdentity {
        manifest_sha256: manifest_sha256.clone(),
        profile_sha256: profile_sha256.clone(),
        priority_reference_year: profile.priority_reference_year,
        priority_policy_version: POLICY_VERSION.to_owned(),
        ranking_digest: digest_paths(
            profile
                .ranking_files
                .iter()
                .chain(profile.alias_files.iter()),
        )?,
        rating_digest: digest_paths(profile.rating_file.iter())?,
    };
    persist_migration_identity(writer.connection(), &migration_identity)?;
    let old_database = options.state_dir.join("corpus.sqlite");
    let runtime_state_migration = if old_database.is_file() {
        Some(migrate_runtime_state(
            &database,
            &old_database,
            &migration_identity,
            unix_seconds() as f64,
        )?)
    } else {
        None
    };

    check_stop(options, "coverage")?;
    phase_start("optimize");
    let started = Instant::now();
    writer
        .connection_mut()
        .execute_batch("ANALYZE; PRAGMA optimize; PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(writer);
    phase_seconds.insert("optimize".to_owned(), started.elapsed().as_secs_f64());

    phase_start("validation");
    let started = Instant::now();
    let database_checks = validate_database(&database)?;
    phase_seconds.insert("validation".to_owned(), started.elapsed().as_secs_f64());
    let snapshot_source = download_dir.join("snapshot.json");
    std::fs::copy(&snapshot_source, run_dir.join("snapshot.json")).map_err(|source| {
        BuildError::Io {
            path: snapshot_source,
            source,
        }
    })?;
    let database_size = file_size(&database)?;
    let database_wal_size = database
        .with_file_name(format!(
            "{}-wal",
            database.file_name().unwrap_or_default().to_string_lossy()
        ))
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let snapshot_path = run_dir.join("snapshot.json");
    let coverage_path = run_dir.join("coverage-initial.json");
    let record_count = checkpoint.completed_records.values().sum::<u64>();
    let elapsed_seconds = phase_seconds.values().sum::<f64>();
    let ingest_seconds = phase_seconds
        .get("ingest")
        .copied()
        .unwrap_or(f64::EPSILON)
        .max(f64::EPSILON);
    let end_to_end_seconds = elapsed_seconds.max(f64::EPSILON);
    let summary = BuildSummary {
        status: "success".to_owned(),
        started_at,
        finished_at: unix_seconds(),
        elapsed_seconds,
        source_count: manifest.sources.len(),
        record_count,
        record_members: source_stats.record_members,
        filtered_members: source_stats.filtered_members,
        records_per_second: record_count as f64 / end_to_end_seconds,
        moves_per_second: ingest.positions as f64 / ingest_seconds,
        positions_per_second: database_checks.game_positions as f64 / end_to_end_seconds,
        sqlite_rows_per_second: database_checks.sqlite_rows as f64 / end_to_end_seconds,
        profile: profile.name.clone(),
        run_id: run_id.to_owned(),
        resumed,
        resumed_records,
        workspace: workspace.display().to_string(),
        accepted: ingest.accepted,
        excluded: ingest.excluded,
        positions: ingest.positions,
        candidates: ingest.candidates,
        ranking_snapshots,
        rating_snapshots,
        unmatched_participants,
        runtime_state_migration,
        database_size,
        database_sha256: sha256_file(&database).map_err(|source| BuildError::Io {
            path: database.clone(),
            source,
        })?,
        database_wal_size,
        snapshot_sha256: sha256_file(&snapshot_path).map_err(|source| BuildError::Io {
            path: snapshot_path,
            source,
        })?,
        coverage_sha256: sha256_file(&coverage_path).map_err(|source| BuildError::Io {
            path: coverage_path,
            source,
        })?,
        input_book_sha256: book_hash,
        profile_sha256,
        priority_policy_version: POLICY_VERSION,
        manifest_sha256,
        download,
        database_checks,
        coverage,
        phase_seconds,
        storage,
        seven_zip,
    };
    write_json(&run_dir.join("corpus-build-summary.json"), &summary)?;

    check_stop(options, "validation")?;
    phase_start("publish");
    for name in ["snapshot.json", "coverage-initial.json"] {
        publish_artifact(&run_dir.join(name), &options.state_dir.join(name))?;
    }
    publish_database_generation(
        &database,
        &options.state_dir.join("corpus.sqlite"),
        &options.state_dir.join("corpus.sqlite.previous"),
    )?;
    publish_artifact(
        &run_dir.join("corpus-build-summary.json"),
        &options.state_dir.join("corpus-build-summary.json"),
    )?;
    match std::fs::remove_file(&checkpoint_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => eprintln!(
            "[corpus] phase=publish event=checkpoint_cleanup_error path={} detail={error}",
            checkpoint_path.display()
        ),
    }
    Ok(summary)
}

fn ingest_profile(
    writer: &mut CorpusWriter,
    profile: &CorpusBuildProfile,
    download_dir: &Path,
    seven_zip_path: Option<&Path>,
    options: &BuildOptions,
    checkpoint_path: &Path,
    checkpoint: &mut BuildCheckpoint,
) -> Result<(IngestBatchResult, u64, SourceVisitStats), BuildError> {
    let mut progress_total = database_ingest_result(writer.connection())?;
    let mut resumed_records = 0_u64;
    let mut source_stats = SourceVisitStats::default();
    let mut ingests: Vec<_> = profile.ingests.iter().collect();
    ingests.sort_by_key(|value| (&value.site, &value.event, value.year));
    for ingest in ingests {
        let priority_key = encode_priority(&priority_tuple(&PriorityFacts {
            year: ingest.year,
            ..PriorityFacts::default()
        })?)?;
        let context = IngestContext {
            site: ingest.site.clone(),
            event: ingest.event.clone(),
            year: ingest.year,
            retrieved_at: ingest.retrieved_at as f64,
            priority_key,
            now: unix_seconds() as f64,
        };
        for input in &ingest.inputs {
            let checkpoint_key = format!("{}/{}/{}", ingest.site, ingest.event, input);
            let completed = checkpoint
                .completed_records
                .get(&checkpoint_key)
                .copied()
                .unwrap_or(0);
            eprintln!(
                "[corpus] phase=ingest event=source_start site={} event_name={} input={} resume_records={completed}",
                ingest.site, ingest.event, input
            );
            let input_path = download_dir.join(Path::new(input));
            let mut batch = Vec::with_capacity(INGEST_BATCH_SIZE);
            let mut visited = 0_u64;
            let result = visit_records(
                &input_path,
                ingest.member_pattern.as_deref(),
                ArchiveLimits::default(),
                seven_zip_path,
                |record| {
                    if stop_requested(options) {
                        return Err(BuildVisitError::Stop);
                    }
                    visited += 1;
                    if visited <= completed {
                        resumed_records += 1;
                        return Ok(());
                    }
                    let source_path = format!("{input}/{}", record.relative_path);
                    let parsed = match record.format {
                        RecordFormat::Csa => {
                            parse_csa(&record.bytes, &source_path).map(|game| vec![game])
                        }
                        RecordFormat::Kif => parse_kif(&record.bytes, &source_path),
                    };
                    match parsed {
                        Ok(games) => batch.extend(games.into_iter().map(RecordOutcome::Accepted)),
                        Err(error) => batch.push(excluded_outcome(
                            source_path,
                            &record.bytes,
                            record.format,
                            error,
                        )),
                    }
                    if batch.len() >= INGEST_BATCH_SIZE {
                        let records = std::mem::take(&mut batch);
                        let result = writer
                            .ingest_batch(&context, records)
                            .map_err(BuildVisitError::Writer)?;
                        add_ingest_result(&mut progress_total, result);
                        checkpoint
                            .completed_records
                            .insert(checkpoint_key.clone(), visited);
                        write_checkpoint(checkpoint_path, checkpoint)
                            .map_err(|error| BuildVisitError::Checkpoint(error.to_string()))?;
                        eprintln!(
                            "[corpus] phase=ingest event=batch_done games={} accepted={} excluded={} positions={} checkpoint_records={visited}",
                            progress_total.accepted + progress_total.excluded,
                            progress_total.accepted,
                            progress_total.excluded,
                            progress_total.positions
                        );
                    }
                    Ok(())
                },
            );
            match result {
                Ok(stats) => {
                    source_stats.record_members += stats.record_members;
                    source_stats.filtered_members += stats.filtered_members;
                    source_stats.visited_records += stats.visited_records;
                }
                Err(VisitRecordsError::Source(error)) => return Err(error.into()),
                Err(VisitRecordsError::Visitor(BuildVisitError::Stop)) => {
                    return Err(BuildError::Stopped {
                        phase: "ingest".to_owned(),
                    });
                }
                Err(VisitRecordsError::Visitor(BuildVisitError::Writer(error))) => {
                    return Err(error.into());
                }
                Err(VisitRecordsError::Visitor(BuildVisitError::Checkpoint(error))) => {
                    return Err(BuildError::Checkpoint(error));
                }
            }
            if visited < completed {
                return Err(BuildError::Checkpoint(format!(
                    "source {checkpoint_key} now has {visited} records but checkpoint requires {completed}"
                )));
            }
            if !batch.is_empty() {
                let result = writer.ingest_batch(&context, std::mem::take(&mut batch))?;
                add_ingest_result(&mut progress_total, result);
            }
            checkpoint.completed_records.insert(checkpoint_key, visited);
            write_checkpoint(checkpoint_path, checkpoint)?;
            eprintln!(
                "[corpus] phase=ingest event=source_done input={input} records={visited} resumed={completed}"
            );
        }
    }
    Ok((
        database_ingest_result(writer.connection())?,
        resumed_records,
        source_stats,
    ))
}

fn database_ingest_result(connection: &Connection) -> Result<IngestBatchResult, BuildError> {
    Ok(IngestBatchResult {
        accepted: connection.query_row("SELECT COUNT(*) FROM source_game", [], |row| row.get(0))?,
        excluded: connection
            .query_row("SELECT COUNT(*) FROM ingest_error", [], |row| row.get(0))?,
        positions: connection
            .query_row("SELECT COUNT(*) FROM game_position", [], |row| row.get(0))?,
        candidates: connection.query_row("SELECT COUNT(*) FROM candidate", [], |row| row.get(0))?,
    })
}
fn excluded_outcome(
    source_path: String,
    bytes: &[u8],
    format: RecordFormat,
    error: RecordError,
) -> RecordOutcome {
    RecordOutcome::Excluded {
        relative_path: source_path,
        sha256: match format {
            RecordFormat::Csa => csa_source_sha256(bytes),
            RecordFormat::Kif => format!("{:x}", Sha256::digest(bytes)),
        },
        error,
    }
}

fn add_ingest_result(total: &mut IngestBatchResult, value: IngestBatchResult) {
    total.accepted += value.accepted;
    total.excluded += value.excluded;
    total.positions += value.positions;
    total.candidates += value.candidates;
}

fn import_rankings(connection: &mut Connection, paths: &[PathBuf]) -> Result<usize, BuildError> {
    for path in paths {
        let document: TournamentRanking = read_json(path)?;
        import_tournament_ranking(connection, &document)?;
    }
    Ok(paths.len())
}

fn import_aliases(connection: &mut Connection, paths: &[PathBuf]) -> Result<usize, BuildError> {
    let mut imported = 0;
    for path in paths {
        let document: ParticipantAliasDocument = read_json(path)?;
        imported += import_participant_aliases(connection, &document)?;
    }
    Ok(imported)
}

fn import_rating(
    connection: &mut Connection,
    profile: &CorpusBuildProfile,
) -> Result<usize, BuildError> {
    let Some(path) = profile.rating_file.as_ref() else {
        return Ok(0);
    };
    let config_path = profile
        .rating_config
        .as_ref()
        .expect("profile validation requires rating config");
    let document: FloodgateRating = read_json(path)?;
    let config = ExtensionConfig::load(config_path)?;
    import_floodgate_rating(
        connection,
        &document,
        RatingPolicy {
            medium_games: config.corpus.rating_medium_games as f64,
            high_games: config.corpus.rating_high_games as f64,
            min_component_size: config.corpus.rating_min_component_size as i32,
        },
    )?;
    Ok(1)
}

fn validate_database(path: &Path) -> Result<DatabaseChecks, BuildError> {
    let connection = Connection::open(path)?;
    connection.execute_batch("PRAGMA query_only=ON; PRAGMA foreign_keys=ON;")?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(BuildError::Validation(format!(
            "integrity_check failed: {integrity}"
        )));
    }
    let foreign_key_error: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()?;
    if let Some(error) = foreign_key_error {
        return Err(BuildError::Validation(format!(
            "foreign_key_check failed: {error}"
        )));
    }
    let schema_version: i64 = connection
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )?
        .parse()
        .map_err(|_| BuildError::Validation("invalid schema version".to_owned()))?;
    if schema_version != 6 {
        return Err(BuildError::Validation(format!(
            "schema version is {schema_version}"
        )));
    }
    let index: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='index'
             AND name='candidate_position_site_quality_idx'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if index.is_none() {
        return Err(BuildError::Validation(
            "candidate_position_site_quality_idx is missing".to_owned(),
        ));
    }
    let invalid_candidates: i64 = connection.query_row(
        "SELECT COUNT(*) FROM candidate
         WHERE quality_band<0 OR source_site NOT BETWEEN 0 AND 3
            OR recent_occurrences<0 OR occurrences<0
            OR recent_occurrences>occurrences",
        [],
        |row| row.get(0),
    )?;
    if invalid_candidates != 0 {
        return Err(BuildError::Validation(format!(
            "candidate quality metadata has {invalid_candidates} invalid rows"
        )));
    }
    let reference_year: i32 = connection
        .query_row(
            "SELECT value FROM meta WHERE key='priority_reference_year'",
            [],
            |row| row.get::<_, String>(0),
        )?
        .parse()
        .map_err(|_| BuildError::Validation("invalid priority reference year".to_owned()))?;
    if reference_year <= 0 {
        return Err(BuildError::Validation(
            "priority reference year must be positive".to_owned(),
        ));
    }
    let policy: String = connection.query_row(
        "SELECT value FROM meta WHERE key='priority_policy_version'",
        [],
        |row| row.get(0),
    )?;
    if policy != POLICY_VERSION {
        return Err(BuildError::Validation(format!(
            "priority policy version is {policy}"
        )));
    }
    let (semantic_games, semantic_positions) = validate_logical_games(&connection)?;
    let mut ingest_error_reasons = BTreeMap::new();
    {
        let mut statement = connection
            .prepare("SELECT reason, COUNT(*) FROM ingest_error GROUP BY reason ORDER BY reason")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (reason, count) = row?;
            ingest_error_reasons.insert(reason, count);
        }
    }
    let mut candidate_query_plan = Vec::new();
    {
        let mut statement = connection.prepare(
            "EXPLAIN QUERY PLAN SELECT id, move FROM candidate
             WHERE position_id=?1 AND active=1 AND source_site=?2 AND quality_band<=?3
             ORDER BY quality_band ASC,priority_key DESC,id LIMIT 1",
        )?;
        let rows = statement.query_map([0_i64, 0_i64, 0_i64], |row| row.get::<_, String>(3))?;
        for row in rows {
            candidate_query_plan.push(row?);
        }
    }
    if !candidate_query_plan
        .iter()
        .any(|detail| detail.contains("candidate_position_site_quality_idx"))
    {
        return Err(BuildError::Validation(format!(
            "candidate lookup does not use candidate_position_site_quality_idx: {candidate_query_plan:?}"
        )));
    }
    Ok(DatabaseChecks {
        integrity_check: integrity,
        foreign_key_check: "ok".to_owned(),
        schema_version,
        raw_sources: count(&connection, "raw_source")?,
        logical_games: count(&connection, "logical_game")?,
        ingest_errors: count(&connection, "ingest_error")?,
        candidates: count(&connection, "candidate")?,
        candidate_sources: connection.query_row(
            "SELECT COUNT(*) FROM game_position gp
             JOIN source_game sg ON sg.game_id=gp.game_id",
            [],
            |row| row.get(0),
        )?,
        positions: count(&connection, "position")?,
        game_positions: count(&connection, "game_position")?,
        sqlite_rows: total_schema_rows(&connection)?,
        semantic_games,
        semantic_positions,
        ingest_error_reasons,
        candidate_query_plan,
    })
}

fn validate_logical_games(connection: &Connection) -> Result<(i64, i64), BuildError> {
    let mut game_statement =
        connection.prepare("SELECT id, initial_sfen, moves_json FROM logical_game ORDER BY id")?;
    let mut position_statement = connection.prepare(
        "SELECT gp.ply, p.position_key, gp.move
         FROM game_position gp JOIN position p ON p.id=gp.position_id
         WHERE gp.game_id=?1 ORDER BY gp.ply",
    )?;
    let mut game_rows = game_statement.query([])?;
    let total_games: i64 =
        connection.query_row("SELECT COUNT(*) FROM logical_game", [], |row| row.get(0))?;
    let validation_started = Instant::now();
    let mut game_count = 0_i64;
    let mut position_count = 0_i64;
    while let Some(game_row) = game_rows.next()? {
        let game_id: i64 = game_row.get(0)?;
        let initial_sfen: String = game_row.get(1)?;
        let moves_json: String = game_row.get(2)?;
        let moves: Vec<String> = serde_json::from_str(&moves_json).map_err(|error| {
            BuildError::Validation(format!("game {game_id} has invalid moves_json: {error}"))
        })?;
        let mut position = parse_position(&initial_sfen).ok_or_else(|| {
            BuildError::Validation(format!("game {game_id} has invalid initial_sfen"))
        })?;
        let mut rows = position_statement.query([game_id])?;
        for (expected_ply, move_usi) in moves.iter().enumerate() {
            let row = rows.next()?.ok_or_else(|| {
                BuildError::Validation(format!(
                    "game {game_id} has no game_position at ply {expected_ply}"
                ))
            })?;
            let ply: i64 = row.get(0)?;
            let stored_position: String = row.get(1)?;
            let stored_move: String = row.get(2)?;
            if ply != expected_ply as i64
                || stored_position != canonical_position_key(&position.to_sfen_owned())
                || stored_move != *move_usi
            {
                return Err(BuildError::Validation(format!(
                    "game {game_id} game_position mismatch at ply {expected_ply}"
                )));
            }
            let move_value = parse_move(move_usi, position.side_to_move()).ok_or_else(|| {
                BuildError::Validation(format!(
                    "game {game_id} has invalid USI move {move_usi} at ply {expected_ply}"
                ))
            })?;
            is_legal_partial(&position, move_value).map_err(|error| {
                BuildError::Validation(format!(
                    "game {game_id} has illegal move {move_usi} at ply {expected_ply}: {error:?}"
                ))
            })?;
            position.make_move(move_value).ok_or_else(|| {
                BuildError::Validation(format!(
                    "game {game_id} cannot apply move {move_usi} at ply {expected_ply}"
                ))
            })?;
            position_count += 1;
        }
        if rows.next()?.is_some() {
            return Err(BuildError::Validation(format!(
                "game {game_id} has excess game_position rows"
            )));
        }
        game_count += 1;
        if game_count % 1_000 == 0 || game_count == total_games {
            let elapsed = validation_started.elapsed().as_secs_f64().max(0.001);
            let games_per_second = game_count as f64 / elapsed;
            let eta_seconds =
                (total_games - game_count).max(0) as f64 / games_per_second.max(f64::EPSILON);
            eprintln!(
                "[corpus] phase=validation event=semantic_progress games={} games_total={} percent={:.2} positions={} games_per_second={:.0} eta_seconds={:.1}",
                game_count,
                total_games,
                100.0 * game_count as f64 / total_games.max(1) as f64,
                position_count,
                games_per_second,
                eta_seconds
            );
        }
    }
    Ok((game_count, position_count))
}

fn canonical_position_key(sfen: &str) -> String {
    sfen.split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}
fn count(connection: &Connection, table: &str) -> Result<i64, rusqlite::Error> {
    connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
}

fn digest_paths<'a>(paths: impl Iterator<Item = &'a PathBuf>) -> Result<String, BuildError> {
    let mut digest = Sha256::new();
    for path in paths {
        digest.update(path.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(sha256_file(path).map_err(|source| BuildError::Io {
            path: path.clone(),
            source,
        })?);
        digest.update([0]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn total_schema_rows(connection: &Connection) -> Result<i64, rusqlite::Error> {
    const TABLES: [&str; 20] = [
        "meta",
        "raw_source",
        "position",
        "logical_game",
        "source_game",
        "game_position",
        "candidate",
        "candidate_adhoc_history",
        "candidate_adhoc_source",
        "search_task",
        "ingest_error",
        "ranking_snapshot",
        "participant",
        "participant_alias",
        "ranking_result",
        "rating_snapshot",
        "rating_result",
        "frontier_history",
        "metric_counter",
        "checkpoint",
    ];
    TABLES
        .iter()
        .try_fold(0_i64, |total, table| Ok(total + count(connection, table)?))
}

fn storage_estimate(manifest: &CorpusManifest, download_dir: &Path) -> StorageEstimate {
    let archive_bytes = manifest.sources.iter().map(|source| source.size).sum();
    let missing_download_bytes = manifest
        .sources
        .iter()
        .filter(|source| {
            !download_dir
                .join(Path::new(&source.relative_path))
                .is_file()
        })
        .map(|source| source.size)
        .sum();
    let estimated_database_bytes = (512 * 1024_u64.pow(2)).max(archive_bytes * 40);
    let safety_bytes = 512 * 1024_u64.pow(2);
    StorageEstimate {
        archive_bytes,
        missing_download_bytes,
        estimated_database_bytes,
        safety_bytes,
        required_free_bytes: estimated_database_bytes + missing_download_bytes + safety_bytes,
        free_bytes_before: 0,
    }
}

#[cfg(windows)]
fn available_space(path: &Path) -> Result<u64, BuildError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let root = path.canonicalize().map_err(|source| BuildError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0_u64;
    // SAFETY: `wide` is NUL-terminated and remains alive for the call; only the valid
    // `available` out-pointer is writable, and both optional out-pointers are null.
    let result = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        Err(BuildError::Io {
            path: root,
            source: std::io::Error::last_os_error(),
        })
    } else {
        Ok(available)
    }
}

#[cfg(not(windows))]
fn available_space(_path: &Path) -> Result<u64, BuildError> {
    Ok(u64::MAX)
}

fn publish_database_generation(
    new_database: &Path,
    active: &Path,
    previous: &Path,
) -> Result<(), BuildError> {
    if !active.exists() {
        replace_file(new_database, active)?;
        return Ok(());
    }
    let previous_temporary = previous.with_file_name(format!(
        "{}.tmp",
        previous.file_name().unwrap_or_default().to_string_lossy()
    ));
    if previous_temporary.exists() {
        std::fs::remove_file(&previous_temporary).map_err(|source| BuildError::Io {
            path: previous_temporary.clone(),
            source,
        })?;
    }
    if std::fs::hard_link(active, &previous_temporary).is_err() {
        std::fs::copy(active, &previous_temporary).map_err(|source| BuildError::Io {
            path: previous_temporary.clone(),
            source,
        })?;
    }
    replace_file(new_database, active)?;
    if let Err(error) = replace_file(&previous_temporary, previous) {
        let rollback_new = replace_file(active, new_database);
        let rollback_old = replace_file(&previous_temporary, active);
        return Err(BuildError::PublishRollback {
            publish: error.to_string(),
            rollback_new: rollback_new.err().map(|value| value.to_string()),
            rollback_old: rollback_old.err().map(|value| value.to_string()),
        });
    }
    Ok(())
}

fn publish_artifact(source: &Path, destination: &Path) -> Result<(), BuildError> {
    let temporary = destination.with_file_name(format!(
        "{}.tmp.{}",
        destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        std::process::id()
    ));
    std::fs::copy(source, &temporary).map_err(|source_error| BuildError::Io {
        path: temporary.clone(),
        source: source_error,
    })?;
    replace_file(&temporary, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), BuildError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both UTF-16 path buffers are NUL-terminated, remain alive for the
    // duration of the call, and are passed read-only to the Windows API.
    let result = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(BuildError::Io {
            path: destination.to_path_buf(),
            source: std::io::Error::last_os_error(),
        })
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), BuildError> {
    std::fs::rename(source, destination).map_err(|source_error| BuildError::Io {
        path: destination.to_path_buf(),
        source: source_error,
    })
}

fn load_checkpoint(
    path: &Path,
    database: &Path,
    profile_sha256: &str,
    manifest_sha256: &str,
    run_id: &str,
) -> Result<(BuildCheckpoint, bool), BuildError> {
    if path.is_file() && database.is_file() {
        let checkpoint: BuildCheckpoint = read_json(path)?;
        if checkpoint.version != 1
            || checkpoint.profile_sha256 != profile_sha256
            || checkpoint.manifest_sha256 != manifest_sha256
        {
            return Err(BuildError::Checkpoint(
                "checkpoint identity does not match profile and manifest".to_owned(),
            ));
        }
        return Ok((checkpoint, true));
    }
    if path.is_file() {
        std::fs::remove_file(path).map_err(|source| BuildError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    }
    if database.is_file() {
        let orphan = database.with_file_name(format!("corpus.sqlite.orphan.{run_id}"));
        std::fs::rename(database, &orphan).map_err(|source| BuildError::Io {
            path: orphan,
            source,
        })?;
    }
    Ok((
        BuildCheckpoint::new(profile_sha256.to_owned(), manifest_sha256.to_owned()),
        false,
    ))
}

fn write_checkpoint(path: &Path, checkpoint: &BuildCheckpoint) -> Result<(), BuildError> {
    let temporary = path.with_file_name(format!(
        "{}.tmp.{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    write_json(&temporary, checkpoint)?;
    replace_file(&temporary, path)
}
#[derive(Serialize)]
struct FailureSummary<'a> {
    status: &'a str,
    profile: &'a str,
    run_id: &'a str,
    started_at: u64,
    failed_at: u64,
    phase: &'a str,
    exit_code: u8,
    detail: String,
}

fn write_failure_summary(
    run_dir: &Path,
    profile: &str,
    run_id: &str,
    started_at: u64,
    error: &BuildError,
) {
    let summary = FailureSummary {
        status: if matches!(error, BuildError::Stopped { .. }) {
            "stopped"
        } else {
            "failed"
        },
        profile,
        run_id,
        started_at,
        failed_at: unix_seconds(),
        phase: error.phase(),
        exit_code: error.exit_code(),
        detail: error.to_string().chars().take(2_000).collect(),
    };
    let path = run_dir.join("corpus-build-summary.json");
    if let Err(summary_error) = write_json(&path, &summary) {
        eprintln!(
            "[corpus] phase=summary event=error path={} detail={summary_error}",
            path.display()
        );
    }
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), BuildError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes).map_err(|source| BuildError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T, BuildError> {
    let bytes = std::fs::read(path).map_err(|source| BuildError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn file_size(path: &Path) -> Result<u64, BuildError> {
    path.metadata()
        .map(|value| value.len())
        .map_err(|source| BuildError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn phase_start(name: &str) {
    eprintln!("[corpus] phase={name} event=start");
}

fn clear_stale_stop_request(state_dir: &Path) -> Result<(), BuildError> {
    let path = state_dir.join("stop.request");
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(BuildError::Io { path, source }),
    }
}

fn stop_requested(options: &BuildOptions) -> bool {
    options.stop.load(Ordering::Relaxed) || options.state_dir.join("stop.request").is_file()
}

fn check_stop(options: &BuildOptions, phase: &str) -> Result<(), BuildError> {
    if stop_requested(options) {
        Err(BuildError::Stopped {
            phase: phase.to_owned(),
        })
    } else {
        Ok(())
    }
}
fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug)]
enum BuildVisitError {
    Stop,
    Checkpoint(String),
    Writer(CorpusWriterError),
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("profile error: {0}")]
    Profile(#[from] crate::corpus_build_profile::ProfileError),
    #[error("lock error: {0}")]
    Lock(#[from] crate::runtime::RuntimeError),
    #[error("download error: {0}")]
    Download(#[from] crate::corpus_download::DownloadError),
    #[error("source error: {0}")]
    Source(#[from] crate::corpus_source::SourceError),
    #[error("writer error: {0}")]
    Writer(#[from] CorpusWriterError),
    #[error("metadata error: {0}")]
    Metadata(#[from] crate::corpus_metadata::MetadataError),
    #[error("coverage error: {0}")]
    Coverage(#[from] crate::corpus_coverage::CoverageError),
    #[error("runtime state migration error: {0}")]
    Migration(#[from] crate::corpus_state_migration::MigrationError),
    #[error("configuration error: {0}")]
    Config(#[from] crate::config::ConfigError),
    #[error("priority error: {0}")]
    Priority(#[from] crate::priority::PriorityError),
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
    #[error("insufficient free space: required={required}, available={available}")]
    InsufficientSpace { required: u64, available: u64 },
    #[error("build stopped during {phase} before publish")]
    Stopped { phase: String },
    #[error("checkpoint error: {0}")]
    Checkpoint(String),
    #[error("database validation failed: {0}")]
    Validation(String),
    #[error(
        "publish failed and rollback was attempted: publish={publish}, rollback_new={rollback_new:?}, rollback_old={rollback_old:?}"
    )]
    PublishRollback {
        publish: String,
        rollback_new: Option<String>,
        rollback_old: Option<String>,
    },
}

impl BuildError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Download(_) => 3,
            Self::Source(_)
            | Self::Writer(_)
            | Self::Metadata(_)
            | Self::Migration(_)
            | Self::Config(_)
            | Self::Priority(_)
            | Self::Sqlite(_) => 4,
            Self::Coverage(_) | Self::Validation(_) | Self::PublishRollback { .. } => 5,
            Self::Checkpoint(_) => 4,
            Self::Lock(_) => 6,
            Self::Stopped { .. } => 130,
            Self::Profile(_) | Self::Json(_) | Self::Io { .. } | Self::InsufficientSpace { .. } => {
                2
            }
        }
    }

    pub fn phase(&self) -> &str {
        match self {
            Self::Profile(_) | Self::Json(_) => "profile",
            Self::Lock(_) => "lock",
            Self::Download(_) => "download",
            Self::Source(_) | Self::Writer(_) => "ingest",
            Self::Metadata(_) | Self::Migration(_) | Self::Config(_) | Self::Priority(_) => {
                "metadata"
            }
            Self::Coverage(_) => "coverage",
            Self::Sqlite(_) => "sqlite",
            Self::Io { .. } => "filesystem",
            Self::InsufficientSpace { .. } => "storage",
            Self::Stopped { phase } => phase,
            Self::Checkpoint(_) => "checkpoint",
            Self::Validation(_) => "validation",
            Self::PublishRollback { .. } => "publish",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::publish_database_generation;

    #[test]
    fn publish_failure_restores_active_database() {
        let directory = tempfile::tempdir().unwrap();
        let new_database = directory.path().join("new.sqlite");
        let active = directory.path().join("corpus.sqlite");
        let previous = directory.path().join("corpus.sqlite.previous");
        std::fs::write(&new_database, b"new generation").unwrap();
        std::fs::write(&active, b"old generation").unwrap();
        std::fs::create_dir(&previous).unwrap();

        let error = publish_database_generation(&new_database, &active, &previous).unwrap_err();
        assert!(error.to_string().contains("rollback"));
        assert_eq!(std::fs::read(&active).unwrap(), b"old generation");
        assert_eq!(std::fs::read(&new_database).unwrap(), b"new generation");
        assert!(previous.is_dir());
    }
}
