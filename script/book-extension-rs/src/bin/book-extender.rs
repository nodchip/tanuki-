use std::{path::PathBuf, process::ExitCode, sync::Arc};

use book_extension_runtime::{
    book::OpeningBook,
    config::ExtensionConfig,
    coordinator::{
        CorpusRuntimeOptions, NormalRuntimeOptions, PersistenceOptions, StopControlOptions,
        WorkerRole, run_normal_extension,
    },
    engine::EngineOptions,
    runtime::BookFileLock,
    storage::sha256_file,
    validation::validate_book,
};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "book-extender")]
struct Args {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
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
    let book = OpeningBook::load(&args.input, args.ignore_ply)?;
    let report = validate_book(&book);
    if !report.valid() {
        return Err(format!("input book has {} validation issues", report.issues.len()).into());
    }
    let black_target = match (&args.black_target, config.workers.vulnerability_black) {
        (Some(path), count) if count > 0 => {
            Some(Arc::new(OpeningBook::load(path, args.ignore_ply)?))
        }
        (None, count) if count > 0 => {
            return Err("vulnerability_black workers require --black-target".into());
        }
        _ => None,
    };
    let white_target = match (&args.white_target, config.workers.vulnerability_white) {
        (Some(path), count) if count > 0 => {
            if args.black_target.as_ref() == Some(path) {
                black_target.clone()
            } else {
                Some(Arc::new(OpeningBook::load(path, args.ignore_ply)?))
            }
        }
        (None, count) if count > 0 => {
            return Err("vulnerability_white workers require --white-target".into());
        }
        _ => None,
    };
    let mut worker_roles = Vec::with_capacity(config.workers.engine_count);
    if let Some(target_book) = black_target {
        worker_roles.extend((0..config.workers.vulnerability_black).map(|_| {
            WorkerRole::Vulnerability {
                target_book: Arc::clone(&target_book),
                target_side: "black",
            }
        }));
    }
    if let Some(target_book) = white_target {
        worker_roles.extend((0..config.workers.vulnerability_white).map(|_| {
            WorkerRole::Vulnerability {
                target_book: Arc::clone(&target_book),
                target_side: "white",
            }
        }));
    }
    worker_roles.extend((0..config.workers.general).map(|_| WorkerRole::General));
    let corpus = if config.corpus.enabled {
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
            input_book_hash: sha256_file(&args.input)?,
            site_weights: [
                config.corpus.wcsc_weight as i64,
                config.corpus.denryu_weight as i64,
                config.corpus.floodgate_weight as i64,
            ],
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
    let extra = args
        .setoptions
        .iter()
        .map(|option| {
            option.split_once('=').map_or_else(
                || (option.clone(), None),
                |(name, value)| (name.to_owned(), Some(value.to_owned())),
            )
        })
        .collect();
    let runtime_options = NormalRuntimeOptions {
        worker_roles,
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
        root_sfen: args.root_sfen.clone(),
        search_timeout_sec: args.usi_search_timeout_sec,
        random_seed: args.random_seed.map(i64::unsigned_abs),
        corpus,
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
