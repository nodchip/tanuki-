use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};

use book_extension_runtime::{
    config::ExtensionConfig,
    coordinator::{
        CorpusRuntimeOptions, NormalRuntimeOptions, PersistenceOptions, StopControlOptions,
        WorkerRole, run_normal_extension,
    },
    engine::EngineOptions,
    engine_fingerprint::{EngineFingerprintInput, compute_engine_fingerprint},
    runtime::BookFileLock,
    sqlite_book::{SqliteOpeningBook, export_yaneuraou_atomic},
    storage::sha256_file,
};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "book-extender")]
struct Args {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    run_id: Option<String>,
    #[arg(long)]
    input: PathBuf,
    #[arg(long = "import-book")]
    import_books: Vec<PathBuf>,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    database: Option<PathBuf>,
    #[arg(long)]
    engine: PathBuf,
    #[arg(long)]
    nodes: u64,
    #[arg(long)]
    multipv: usize,
    #[arg(long, default_value_t = 1024)]
    usi_hash: usize,
    #[arg(long = "setoption")]
    setoptions: Vec<String>,
    #[arg(long, default_value_t = 1.4)]
    c_puct: f64,
    #[arg(long, default_value_t = 600.0)]
    eval_scale: f64,
    #[arg(long)]
    eval_diff: Option<i32>,
    #[arg(long)]
    min_eval_cp: Option<i32>,
    #[arg(long, default_value = "black")]
    book_side: String,
    #[arg(long, default_value_t = 200)]
    max_ply: usize,
    #[arg(long, default_value_t = 3600.0)]
    usi_search_timeout_sec: f64,
    #[arg(long, default_value = book_extension_runtime::STARTPOS_SFEN)]
    root_sfen: String,
    #[arg(long)]
    max_searches: Option<u64>,
    #[arg(long)]
    max_added_positions: Option<u64>,
    #[arg(long)]
    max_total_nodes: Option<u64>,
    #[arg(long)]
    max_runtime_sec: Option<f64>,
    #[arg(long)]
    random_seed: Option<i64>,
    #[arg(long, default_value_t = false)]
    ignore_ply: bool,
    #[arg(long)]
    heartbeat_path: Option<PathBuf>,
    #[arg(long)]
    stop_request_path: Option<PathBuf>,
    #[arg(long)]
    lock_path: Option<PathBuf>,
    #[arg(long)]
    black_target: Option<PathBuf>,
    #[arg(long)]
    white_target: Option<PathBuf>,
    #[arg(long)]
    corpus_db: Option<PathBuf>,
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.nodes == 0 || args.multipv == 0 || args.usi_search_timeout_sec <= 0.0 {
        return Err("nodes, multipv, and usi-search-timeout-sec must be positive".into());
    }
    let config = ExtensionConfig::load(&args.config)?;
    let lock_path = args
        .lock_path
        .unwrap_or_else(|| config.runtime.state_dir.join("book-extension.lock"));
    let _lock = BookFileLock::acquire(&lock_path, &format!(r#"{{"pid":{}}}"#, std::process::id()))?;
    let stop_request_path = args
        .stop_request_path
        .clone()
        .unwrap_or_else(|| config.runtime.state_dir.join("stop.request"));
    match std::fs::remove_file(&stop_request_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot clear stale stop request: {error}").into()),
    }
    let database_path = args.database.clone().unwrap_or_else(|| {
        let output_name = args
            .output
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        config
            .runtime
            .state_dir
            .join(format!("{output_name}.sqlite"))
    });
    let mut book = SqliteOpeningBook::open(&database_path, args.ignore_ply)?;
    let input_report = book.import_yaneuraou_compatible(&args.input)?;
    eprintln!(
        "[book_import] source={} sha256={} already_imported={} positions={} moves={}",
        args.input.display(),
        input_report.input_sha256,
        input_report.already_imported,
        input_report.inserted_positions,
        input_report.inserted_moves
    );
    for target in args.import_books.iter().chain(
        [&args.black_target, &args.white_target]
            .into_iter()
            .flatten(),
    ) {
        let report = book.import_yaneuraou(target)?;
        eprintln!(
            "[book_import] source={} sha256={} already_imported={} positions={} moves={}",
            target.display(),
            report.input_sha256,
            report.already_imported,
            report.inserted_positions,
            report.inserted_moves
        );
    }
    export_yaneuraou_atomic(&database_path, &args.output, config.runtime.backup_count)?;
    let mut worker_roles = Vec::with_capacity(config.workers.engine_count);
    worker_roles.extend((0..config.workers.fixed_black).map(|_| WorkerRole::FixedBlack));
    worker_roles.extend((0..config.workers.fixed_white).map(|_| WorkerRole::FixedWhite));
    worker_roles.extend((0..config.workers.general).map(|_| WorkerRole::General));
    let mut corpus = if config.corpus.enabled {
        let database_path = args
            .corpus_db
            .clone()
            .ok_or("enabled corpus requires --corpus-db")?;
        let share = config.corpus.general_pool_node_share;
        let corpus_nodes = if share < 1.0 {
            ((args.nodes as f64 * share / (1.0 - share)).round() as u64).max(1)
        } else {
            args.nodes
        };
        Some(CorpusRuntimeOptions {
            database_path,
            snapshot_id: args
                .output
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            nodes: corpus_nodes,
            max_concurrent: config.corpus.max_concurrent_searches,
            saturation_window: config.corpus.saturation_window as i64,
            input_book_hash: sha256_file(&args.output)?,
            site_weights: [
                config.corpus.wcsc_weight as i64,
                config.corpus.denryu_weight as i64,
                config.corpus.floodgate_weight as i64,
            ],
            engine_fingerprint: String::new(),
        })
    } else {
        None
    };
    if args.eval_diff.is_some_and(|value| value < 0) || args.eval_scale <= 0.0 {
        return Err("eval-diff must be non-negative and eval-scale must be positive".into());
    }
    let book_side = match args.book_side.as_str() {
        "black" => "black",
        "white" => "white",
        _ => return Err("book-side must be black or white".into()),
    };
    let extra: Vec<(String, Option<String>)> = args
        .setoptions
        .iter()
        .map(|option| {
            option.split_once('=').map_or_else(
                || (option.clone(), None),
                |(name, value)| (name.to_owned(), Some(value.to_owned())),
            )
        })
        .collect();
    let fingerprint = compute_engine_fingerprint(&EngineFingerprintInput {
        engine_sha256: sha256_file(&args.engine)?,
        hash_mb: args.usi_hash,
        threads: config.workers.threads_per_engine,
        multipv: args.multipv,
        extra: extra.iter().cloned().collect::<BTreeMap<_, _>>(),
        corpus_nodes: corpus.as_ref().map_or(0, |value| value.nodes),
        search_timeout_sec: args.usi_search_timeout_sec,
        engine_config_revision: config.runtime.engine_config_revision,
    })?;
    if let Some(corpus) = corpus.as_mut() {
        corpus.engine_fingerprint = fingerprint.clone();
    }
    if args
        .run_id
        .as_ref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err("run-id must not be empty".into());
    }
    let run_id = args.run_id.unwrap_or_else(|| {
        format!(
            "direct-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        )
    });
    let runtime_options = NormalRuntimeOptions {
        worker_roles,
        run_id,
        engine_options: EngineOptions {
            hash_mb: args.usi_hash,
            threads: config.workers.threads_per_engine,
            multipv: args.multipv,
            extra,
        },
        nodes: args.nodes,
        multipv: args.multipv,
        max_searches: args.max_searches.unwrap_or(u64::MAX),
        max_added_positions: args.max_added_positions,
        max_total_nodes: args.max_total_nodes,
        max_ply: Some(args.max_ply),
        c_puct: args.c_puct,
        eval_scale: args.eval_scale,
        book_side,
        eval_diff: args.eval_diff,
        min_eval_cp: args.min_eval_cp,
        root_sfen: args.root_sfen.clone(),
        search_timeout_sec: args.usi_search_timeout_sec,
        random_seed: args.random_seed.map(i64::unsigned_abs),
        corpus,
        status_path: config.runtime.state_dir.join("runtime-status.json"),
        status_interval_sec: config.runtime.status_interval_sec,
        engine_fingerprint: fingerprint,
        persistence: PersistenceOptions {
            output_path: args.output.clone(),
            backup_count: config.runtime.backup_count,
            save_interval_sec: config.runtime.save_interval_sec,
        },
        stop_control: Some(StopControlOptions {
            heartbeat_path: args.heartbeat_path.clone(),
            stop_request_path: Some(stop_request_path),
            heartbeat_timeout_sec: config.runtime.heartbeat_timeout_sec,
            max_runtime_sec: args.max_runtime_sec,
            usi_stop_timeout_sec: config.runtime.usi_stop_timeout_sec,
        }),
    };
    let (_book, report) = run_normal_extension(book, &args.engine, &runtime_options)?;
    eprintln!(
        "[runtime] engines={} searches={} added_positions={}",
        report.engines, report.searches, report.added_positions
    );
    eprintln!(
        "[stop] reason={} output={}",
        report.stop_reason.as_deref().unwrap_or("max-searches"),
        args.output.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("book-extender: {error}");
            ExitCode::from(1)
        }
    }
}
