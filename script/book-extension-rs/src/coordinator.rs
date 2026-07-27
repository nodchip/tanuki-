use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use thiserror::Error;

use crate::{
    corpus::{
        CandidateChoice, CorpusCandidate, CorpusStore, FrontierTransition, RolloutObservation,
        SearchCompletion, SearchTaskStatus, SourceSite,
    },
    engine::{EngineError, EngineOptions, StopHandle, UsiEngine},
    priority::SiteNodeBudget,
    python_random::PythonRandom,
    runtime::HeartbeatMonitor,
    runtime_status::{
        BookSaveStatus, LastCandidateStatus, LastSearchStatus, RuntimeStatusSnapshot,
        TaskStatusCounts, write_runtime_status_atomic,
    },
    search::{LeafPath, PetaFilter, SearchError, reserve_leaf_path_with_filter_and_random},
    sqlite_book::{SqliteOpeningBook, export_yaneuraou_atomic},
    storage::sha256_file,
    usi::PositionRoot,
};

#[derive(Clone, Debug)]
pub enum WorkerRole {
    General,
    FixedBlack,
    FixedWhite,
}

#[derive(Clone, Debug)]
pub struct CorpusRuntimeOptions {
    pub database_path: PathBuf,
    pub snapshot_id: String,
    pub nodes: u64,
    pub max_concurrent: usize,
    pub saturation_window: i64,
    pub input_book_hash: String,
    pub site_weights: [i64; 3],
    pub engine_fingerprint: String,
}

#[derive(Clone, Debug)]
pub struct PersistenceOptions {
    pub output_path: PathBuf,
    pub backup_count: usize,
    pub save_interval_sec: f64,
}

#[derive(Clone, Debug)]
pub struct StopControlOptions {
    pub heartbeat_path: Option<PathBuf>,
    pub stop_request_path: Option<PathBuf>,
    pub heartbeat_timeout_sec: f64,
    pub max_runtime_sec: Option<f64>,
    pub usi_stop_timeout_sec: f64,
}

#[derive(Clone, Debug)]
pub struct NormalRuntimeOptions {
    pub worker_roles: Vec<WorkerRole>,
    pub engine_options: EngineOptions,
    pub nodes: u64,
    pub multipv: usize,
    pub max_searches: u64,
    pub max_added_positions: Option<u64>,
    pub max_total_nodes: Option<u64>,
    pub max_ply: Option<usize>,
    pub c_puct: f64,
    pub eval_scale: f64,
    pub book_side: &'static str,
    pub eval_diff: Option<i32>,
    pub min_eval_cp: Option<i32>,
    pub root_sfen: String,
    pub search_timeout_sec: f64,
    pub random_seed: Option<u64>,
    pub corpus: Option<CorpusRuntimeOptions>,
    pub stop_control: Option<StopControlOptions>,
    pub persistence: PersistenceOptions,
    pub status_path: PathBuf,
    pub status_interval_sec: f64,
    pub engine_fingerprint: String,
    pub run_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalRuntimeReport {
    pub engines: usize,
    pub searches: u64,
    pub added_positions: u64,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Error)]
pub enum CoordinatorError {
    #[error("worker_count, nodes, multipv, and max_searches must be positive")]
    InvalidOptions,
    #[error("USI engine failed: {0}")]
    Engine(#[from] EngineError),
    #[error("search failed: {0}")]
    Search(#[from] SearchError),
    #[error("runtime state lock is poisoned")]
    Poisoned,
    #[error("worker failed: {0}")]
    Worker(String),
    #[error("worker thread panicked")]
    WorkerPanic,
    #[error("runtime state still has multiple owners")]
    StateStillShared,
}

struct SharedState {
    book: SqliteOpeningBook,
    run_id: String,
    started_at: f64,
    inflight: HashSet<String>,
    searches: u64,
    total_nodes: u64,
    added_positions: u64,
    stop_admission: bool,
    error: Option<String>,
    corpus_store: Option<CorpusStore>,
    corpus_active: usize,
    discard_results: bool,
    stop_reason: Option<String>,
    running_workers: usize,
    site_budget: Option<SiteNodeBudget>,
    generation: u64,
    corpus_add_successes: i64,
    last_candidate: Option<LastCandidateStatus>,
    lane_searches: BTreeMap<String, u64>,
    lane_active: BTreeMap<String, usize>,
    last_search: Option<LastSearchStatus>,
    last_book_save: Option<BookSaveStatus>,
    save_finished: bool,
    random: PythonRandom,
}

struct CorpusWork {
    candidate: CorpusCandidate,
    sfen: String,
    history: Vec<String>,
    path: LeafPath,
}

pub fn run_normal_extension(
    mut book: SqliteOpeningBook,
    engine_path: &Path,
    options: &NormalRuntimeOptions,
) -> Result<(SqliteOpeningBook, NormalRuntimeReport), CoordinatorError> {
    let runtime_started = std::time::Instant::now();
    if options.worker_roles.is_empty()
        || options.nodes == 0
        || options.multipv == 0
        || options.max_searches == 0
        || options
            .corpus
            .as_ref()
            .is_some_and(|corpus| corpus.nodes == 0 || corpus.max_concurrent == 0)
    {
        return Err(CoordinatorError::InvalidOptions);
    }
    let corpus_store = if let Some(corpus) = &options.corpus {
        let mut store = CorpusStore::open(&corpus.database_path)
            .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
        store
            .reset_interrupted_tasks(unix_time())
            .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
        store
            .requeue_retryable_failures(&corpus.engine_fingerprint, unix_time())
            .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
        replay_corpus_results(&mut book, &store, &corpus.input_book_hash)?;
        Some(store)
    } else {
        None
    };
    let mut engines = Vec::with_capacity(options.worker_roles.len());
    for _ in 0..options.worker_roles.len() {
        engines.push(UsiEngine::start(
            engine_path,
            options.engine_options.clone(),
        )?);
    }
    let stop_handles: Vec<StopHandle> = engines.iter().map(UsiEngine::stop_handle).collect();
    let lane_searches: BTreeMap<String, u64> = options
        .worker_roles
        .iter()
        .map(|role| (worker_role_label(role).to_owned(), 0))
        .collect();
    let lane_active = lane_searches.keys().map(|lane| (lane.clone(), 0)).collect();
    let shared = Arc::new((
        Mutex::new(SharedState {
            book,
            run_id: options.run_id.clone(),
            started_at: unix_time(),
            inflight: HashSet::new(),
            searches: 0,
            total_nodes: 0,
            added_positions: 0,
            stop_admission: false,
            error: None,
            corpus_store,
            corpus_active: 0,
            discard_results: false,
            stop_reason: None,
            running_workers: options.worker_roles.len(),
            generation: 0,
            corpus_add_successes: 0,
            last_candidate: None,
            lane_searches,
            lane_active,
            last_search: None,
            last_book_save: None,
            save_finished: false,
            random: PythonRandom::seeded(options.random_seed.unwrap_or_else(default_random_seed)),
            site_budget: options.corpus.as_ref().map(|corpus| {
                SiteNodeBudget::new([
                    ("wcsc", corpus.site_weights[0]),
                    ("denryu", corpus.site_weights[1]),
                    ("floodgate", corpus.site_weights[2]),
                ])
                .expect("validated config weights")
            }),
        }),
        Condvar::new(),
    ));

    let saver_handle = {
        let shared = Arc::clone(&shared);
        let persistence = options.persistence.clone();
        thread::spawn(move || -> Result<(), CoordinatorError> {
            let interval = std::time::Duration::from_secs_f64(persistence.save_interval_sec);
            let mut last_saved = None;
            let mut next_save = std::time::Instant::now() + interval;
            'save_loop: loop {
                let (database_path, generation, through_task, workers_done) = {
                    let (state_lock, wake) = &*shared;
                    let mut state = state_lock.lock().map_err(|_| CoordinatorError::Poisoned)?;
                    while state.running_workers != 0 && std::time::Instant::now() < next_save {
                        let remaining =
                            next_save.saturating_duration_since(std::time::Instant::now());
                        state = wake
                            .wait_timeout(state, remaining)
                            .map_err(|_| CoordinatorError::Poisoned)?
                            .0;
                    }
                    if last_saved == Some(state.generation) {
                        if state.running_workers == 0 {
                            return Ok(());
                        }
                        next_save = std::time::Instant::now() + interval;
                        continue 'save_loop;
                    }
                    let through_task = state
                        .corpus_store
                        .as_ref()
                        .map(|store| store.latest_evaluated_task_id())
                        .transpose()
                        .map_err(|error| CoordinatorError::Worker(error.to_string()))?
                        .flatten();
                    (
                        state.book.path().to_path_buf(),
                        state.generation,
                        through_task,
                        state.running_workers == 0,
                    )
                };
                let save_started = std::time::Instant::now();
                if let Err(error) = export_yaneuraou_atomic(
                    &database_path,
                    &persistence.output_path,
                    persistence.backup_count,
                ) {
                    let mut state = shared.0.lock().map_err(|_| CoordinatorError::Poisoned)?;
                    state.last_book_save = Some(BookSaveStatus {
                        saved_at: unix_time(),
                        path: persistence.output_path.display().to_string(),
                        success: false,
                        detail: error.to_string(),
                    });
                    eprintln!(
                        "[book_save] generation={} elapsed_ms={} output={} status=failure error={:?}",
                        generation,
                        save_started.elapsed().as_millis(),
                        persistence.output_path.display(),
                        error.to_string()
                    );
                    state.error = Some(error.to_string());
                    state.stop_admission = true;
                    shared.1.notify_all();
                    return Err(CoordinatorError::Worker(error.to_string()));
                }
                let hash = sha256_file(&persistence.output_path)
                    .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
                {
                    let mut state = shared.0.lock().map_err(|_| CoordinatorError::Poisoned)?;
                    if let Some(store) = state.corpus_store.as_mut() {
                        store
                            .record_checkpoint_through(&hash, through_task, unix_time())
                            .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
                    }
                    state.last_book_save = Some(BookSaveStatus {
                        saved_at: unix_time(),
                        path: persistence.output_path.display().to_string(),
                        success: true,
                        detail: hash.clone(),
                    });
                }
                eprintln!(
                    "[book_save] generation={} through_task={} elapsed_ms={} output={} status=success",
                    generation,
                    through_task.map_or_else(|| "none".to_owned(), |id| id.to_string()),
                    save_started.elapsed().as_millis(),
                    persistence.output_path.display()
                );
                last_saved = Some(generation);
                next_save = std::time::Instant::now() + interval;
                if workers_done {
                    let state = shared.0.lock().map_err(|_| CoordinatorError::Poisoned)?;
                    if state.generation == generation {
                        return Ok(());
                    }
                }
            }
        })
    };

    let status_handle = {
        let shared = Arc::clone(&shared);
        let status_path = options.status_path.clone();
        let status_interval = std::time::Duration::from_secs_f64(options.status_interval_sec);
        let engine_fingerprint = options.engine_fingerprint.clone();
        thread::spawn(move || -> Result<(), CoordinatorError> {
            let mut consecutive_failures = 0_u64;
            loop {
                let (snapshot_result, workers_done) = {
                    let state = shared.0.lock().map_err(|_| CoordinatorError::Poisoned)?;
                    (
                        runtime_status_snapshot(&state, &engine_fingerprint),
                        state.running_workers == 0 && state.save_finished,
                    )
                };
                let write_result = snapshot_result.and_then(|snapshot| {
                    write_runtime_status_atomic(&status_path, &snapshot)
                        .map_err(|error| CoordinatorError::Worker(error.to_string()))
                });
                match write_result {
                    Ok(()) => {
                        if consecutive_failures > 0 {
                            eprintln!(
                                "[runtime-status] event=recovered failures={} path={}",
                                consecutive_failures,
                                status_path.display()
                            );
                        }
                        consecutive_failures = 0;
                    }
                    Err(error) => {
                        consecutive_failures += 1;
                        eprintln!(
                            "[runtime-status] event=write-failed failures={} path={} error={:?}",
                            consecutive_failures,
                            status_path.display(),
                            error.to_string()
                        );
                    }
                }
                if workers_done {
                    return Ok(());
                }
                let state = shared.0.lock().map_err(|_| CoordinatorError::Poisoned)?;
                if state.running_workers != 0 {
                    let _ = shared
                        .1
                        .wait_timeout(state, status_interval)
                        .map_err(|_| CoordinatorError::Poisoned)?;
                }
            }
        })
    };
    if let Some(control) = &options.stop_control
        && let Some(heartbeat_path) = control.heartbeat_path.as_deref()
    {
        let monitor = HeartbeatMonitor::new(
            heartbeat_path,
            control.heartbeat_timeout_sec,
            unix_time(),
            control.stop_request_path.clone(),
        )
        .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
        if let Some(reason) = monitor.stop_reason(unix_time()) {
            let mut state = shared.0.lock().map_err(|_| CoordinatorError::Poisoned)?;
            state.stop_admission = true;
            state.discard_results = true;
            state.stop_reason = Some(reason.to_owned());
        }
    }
    let monitor_handle = options.stop_control.clone().map(|control| {
        let shared = Arc::clone(&shared);
        thread::spawn(move || -> Result<(), CoordinatorError> {
            let runtime_started = std::time::Instant::now();
            let monitor = HeartbeatMonitor::new(
                control
                    .heartbeat_path
                    .as_deref()
                    .unwrap_or_else(|| Path::new("__heartbeat-disabled__")),
                control.heartbeat_timeout_sec,
                unix_time(),
                control.stop_request_path,
            )
            .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
            loop {
                let reason = {
                    let state = shared.0.lock().map_err(|_| CoordinatorError::Poisoned)?;
                    if state.running_workers == 0 {
                        return Ok(());
                    }
                    if control.heartbeat_path.is_none() {
                        if monitor.stop_reason(unix_time()) == Some("manual-stop-request") {
                            Some("manual-stop-request")
                        } else {
                            None
                        }
                    } else {
                        monitor.stop_reason(unix_time())
                    }
                    .or_else(|| {
                        control
                            .max_runtime_sec
                            .filter(|limit| runtime_started.elapsed().as_secs_f64() >= *limit)
                            .map(|_| "max-runtime-sec")
                    })
                };
                if let Some(reason) = reason {
                    let stop_started = std::time::Instant::now();
                    eprintln!("[stop-request] reason={reason}");
                    eprintln!(
                        "[runtime_stop] reason={} elapsed_ms={}",
                        reason,
                        runtime_started.elapsed().as_millis()
                    );
                    {
                        let mut state = shared.0.lock().map_err(|_| CoordinatorError::Poisoned)?;
                        state.stop_admission = true;
                        state.discard_results = true;
                        state.stop_reason = Some(reason.to_owned());
                        shared.1.notify_all();
                    }
                    for handle in &stop_handles {
                        let _ = handle.stop();
                    }
                    let deadline = std::time::Instant::now()
                        + std::time::Duration::from_secs_f64(control.usi_stop_timeout_sec);
                    while stop_handles.iter().any(StopHandle::is_searching)
                        && std::time::Instant::now() < deadline
                    {
                        thread::sleep(std::time::Duration::from_millis(10));
                    }
                    let mut forced = 0;
                    for handle in &stop_handles {
                        if handle.is_searching() {
                            forced += 1;
                            let _ = handle.force_kill();
                        }
                    }
                    eprintln!(
                        "[stop-usi] reason={} forced={} elapsed_ms={}",
                        reason,
                        forced,
                        stop_started.elapsed().as_millis()
                    );
                    return Ok(());
                }
                thread::sleep(std::time::Duration::from_millis(20));
            }
        })
    });

    let mut handles = Vec::with_capacity(options.worker_roles.len());
    let engine_path = engine_path.to_path_buf();
    for (mut engine, role) in engines
        .into_iter()
        .zip(options.worker_roles.iter().cloned())
    {
        let shared = Arc::clone(&shared);
        let options = options.clone();
        let engine_path = engine_path.clone();
        handles.push(thread::spawn(move || -> Result<(), CoordinatorError> {
            let _done = WorkerDoneGuard(Arc::clone(&shared));
            loop {
                let path = reserve_normal_work(&shared, &options, &role)?;
                let Some(path) = path else { break };
                let history: Vec<String> = path
                    .steps
                    .iter()
                    .map(|step| step.move_usi.clone())
                    .collect();
                let depth = path.steps.len();
                let search_started = std::time::Instant::now();
                eprintln!(
                    "[search-start] lane={} depth={} position_key={} nodes={}",
                    worker_role_label(&role),
                    depth,
                    path.leaf_sfen,
                    options.nodes
                );
                let history_refs: Vec<&str> = history.iter().map(String::as_str).collect();
                let root = position_root(&options.root_sfen);
                let normal_result = search_with_retries(
                    &mut engine,
                    &engine_path,
                    &options,
                    &shared,
                    &root,
                    &history_refs,
                    options.nodes,
                );
                let normal_elapsed_ms = search_started.elapsed().as_millis();
                eprintln!(
                    "[search-finish] lane={} depth={} position_key={} status={} elapsed_ms={}",
                    worker_role_label(&role),
                    depth,
                    path.leaf_sfen,
                    if normal_result.is_ok() { "ok" } else { "error" },
                    normal_elapsed_ms
                );

                let corpus_work = {
                    let (state_lock, wake) = &*shared;
                    let mut state = state_lock.lock().map_err(|_| CoordinatorError::Poisoned)?;
                    state.inflight.remove(&path.leaf_sfen);
                    let lane = worker_role_label(&role).to_owned();
                    if let Some(active) = state.lane_active.get_mut(&lane) {
                        *active = active.saturating_sub(1);
                    }
                    state.last_search = Some(LastSearchStatus {
                        lane,
                        depth,
                        position_key: path.leaf_sfen.clone(),
                        status: if normal_result.is_ok() { "ok" } else { "error" }.to_owned(),
                        elapsed_ms: normal_elapsed_ms,
                    });
                    match normal_result {
                        Ok(results) if !state.discard_results => {
                            if !results.is_empty() {
                                apply_normal_results(&mut state, &path, &results)?;
                            }
                        }
                        Ok(_) => {}
                        Err(error) if state.discard_results => {
                            eprintln!(
                                "[engine-error] phase=search action=ignored-during-stop error={:?}",
                                error.to_string()
                            );
                        }
                        Err(error) => {
                            state.error = Some(error.to_string());
                            state.stop_admission = true;
                        }
                    }
                    let work = if state.error.is_none()
                        && !state.discard_results
                        && matches!(role, WorkerRole::General)
                    {
                        reserve_corpus_work(&mut state, &path, &options)?
                    } else {
                        None
                    };
                    wake.notify_all();
                    work
                };

                if let Some(work) = corpus_work {
                    let history_refs: Vec<&str> = work.history.iter().map(String::as_str).collect();
                    let corpus_search_started = std::time::Instant::now();
                    let result = watched_search_move(
                        &mut engine,
                        &position_root(&options.root_sfen),
                        &history_refs,
                        options.corpus.as_ref().expect("work requires corpus").nodes,
                        &work.candidate.move_usi,
                        &options,
                    );
                    if result.is_err() {
                        let stopping = shared
                            .0
                            .lock()
                            .map_err(|_| CoordinatorError::Poisoned)?
                            .discard_results;
                        if !stopping {
                            let _ = engine.restart(&engine_path, options.engine_options.clone());
                        }
                    }
                    let (state_lock, wake) = &*shared;
                    let mut state = state_lock.lock().map_err(|_| CoordinatorError::Poisoned)?;
                    state.corpus_active -= 1;
                    apply_corpus_result(
                        &mut state,
                        &work,
                        result,
                        &options,
                        corpus_search_started.elapsed().as_millis(),
                    )?;
                    wake.notify_all();
                }

                let state = shared.0.lock().map_err(|_| CoordinatorError::Poisoned)?;
                if state.error.is_some() {
                    break;
                }
            }
            let stopping = shared
                .0
                .lock()
                .map_err(|_| CoordinatorError::Poisoned)?
                .discard_results;
            if let Err(error) = engine.close() {
                if stopping {
                    eprintln!(
                        "[engine-error] phase=close action=ignored-during-stop error={:?}",
                        error.to_string()
                    );
                } else {
                    return Err(error.into());
                }
            }
            Ok(())
        }));
    }
    let phase_started = std::time::Instant::now();
    for handle in handles {
        handle.join().map_err(|_| CoordinatorError::WorkerPanic)??;
    }
    eprintln!(
        "[shutdown] phase=worker-join elapsed_ms={}",
        phase_started.elapsed().as_millis()
    );
    let phase_started = std::time::Instant::now();
    saver_handle
        .join()
        .map_err(|_| CoordinatorError::WorkerPanic)??;
    {
        let mut state = shared.0.lock().map_err(|_| CoordinatorError::Poisoned)?;
        state.save_finished = true;
        shared.1.notify_all();
    }
    eprintln!(
        "[shutdown] phase=final-save-checkpoint elapsed_ms={}",
        phase_started.elapsed().as_millis()
    );
    let phase_started = std::time::Instant::now();
    status_handle
        .join()
        .map_err(|_| CoordinatorError::WorkerPanic)??;
    eprintln!(
        "[shutdown] phase=status-join elapsed_ms={}",
        phase_started.elapsed().as_millis()
    );
    let phase_started = std::time::Instant::now();
    if let Some(handle) = monitor_handle {
        handle.join().map_err(|_| CoordinatorError::WorkerPanic)??;
    }
    eprintln!(
        "[shutdown] phase=stop-monitor-join elapsed_ms={} total_ms={}",
        phase_started.elapsed().as_millis(),
        runtime_started.elapsed().as_millis()
    );
    let (state_lock, _) =
        Arc::try_unwrap(shared).map_err(|_| CoordinatorError::StateStillShared)?;
    let state = state_lock
        .into_inner()
        .map_err(|_| CoordinatorError::Poisoned)?;
    if let Some(error) = state.error {
        return Err(CoordinatorError::Worker(error));
    }
    let report = NormalRuntimeReport {
        engines: options.worker_roles.len(),
        searches: state.searches,
        added_positions: state.added_positions,
        stop_reason: state.stop_reason.clone(),
    };
    Ok((state.book, report))
}

struct SearchWatchdog {
    done: std::sync::mpsc::Sender<()>,
    timed_out: Arc<std::sync::atomic::AtomicBool>,
    thread: thread::JoinHandle<()>,
    timeout_sec: f64,
}

impl SearchWatchdog {
    fn start(handle: StopHandle, timeout_sec: f64, stop_timeout_sec: f64) -> Self {
        use std::sync::atomic::Ordering;
        let (done, receiver) = std::sync::mpsc::channel();
        let timed_out = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let timeout_flag = Arc::clone(&timed_out);
        let thread = thread::spawn(move || {
            if matches!(
                receiver.recv_timeout(std::time::Duration::from_secs_f64(timeout_sec)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                timeout_flag.store(true, Ordering::Release);
                let _ = handle.stop();
                let deadline = std::time::Instant::now()
                    + std::time::Duration::from_secs_f64(stop_timeout_sec);
                while handle.is_searching() && std::time::Instant::now() < deadline {
                    thread::sleep(std::time::Duration::from_millis(10));
                }
                if handle.is_searching() {
                    let _ = handle.force_kill();
                }
            }
        });
        Self {
            done,
            timed_out,
            thread,
            timeout_sec,
        }
    }

    fn finish<T>(self, result: Result<T, EngineError>) -> Result<T, EngineError> {
        use std::sync::atomic::Ordering;
        let _ = self.done.send(());
        self.thread.join().map_err(|_| EngineError::Poisoned)?;
        if self.timed_out.load(Ordering::Acquire) {
            Err(EngineError::SearchTimeout(self.timeout_sec))
        } else {
            result
        }
    }
}

fn watchdog_stop_timeout(options: &NormalRuntimeOptions) -> f64 {
    options
        .stop_control
        .as_ref()
        .map_or(5.0, |control| control.usi_stop_timeout_sec)
}

fn watched_search(
    engine: &mut UsiEngine,
    root: &PositionRoot,
    history: &[&str],
    nodes: u64,
    options: &NormalRuntimeOptions,
) -> Result<Vec<crate::search::SearchResult>, EngineError> {
    let watchdog = SearchWatchdog::start(
        engine.stop_handle(),
        options.search_timeout_sec,
        watchdog_stop_timeout(options),
    );
    watchdog.finish(engine.search(root, history, nodes))
}

fn watched_search_move(
    engine: &mut UsiEngine,
    root: &PositionRoot,
    history: &[&str],
    nodes: u64,
    move_usi: &str,
    options: &NormalRuntimeOptions,
) -> Result<crate::search::SearchResult, EngineError> {
    let watchdog = SearchWatchdog::start(
        engine.stop_handle(),
        options.search_timeout_sec,
        watchdog_stop_timeout(options),
    );
    watchdog.finish(engine.search_move(root, history, nodes, move_usi))
}
#[allow(clippy::too_many_arguments)]
fn search_with_retries(
    engine: &mut UsiEngine,
    engine_path: &Path,
    options: &NormalRuntimeOptions,
    shared: &Arc<(Mutex<SharedState>, Condvar)>,
    root: &PositionRoot,
    history: &[&str],
    nodes: u64,
) -> Result<Vec<crate::search::SearchResult>, EngineError> {
    let mut result = watched_search(engine, root, history, nodes, options);
    for retry in 1..3 {
        if result.is_ok() {
            break;
        }
        let stopping = shared
            .0
            .lock()
            .map_err(|_| EngineError::Poisoned)?
            .discard_results;
        if stopping {
            break;
        }
        eprintln!(
            "[engine-retry] attempt={} reason={}",
            retry + 1,
            result.as_ref().unwrap_err()
        );
        engine.restart(engine_path, options.engine_options.clone())?;
        result = watched_search(engine, root, history, nodes, options);
    }
    result
}
fn worker_role_label(role: &WorkerRole) -> &'static str {
    match role {
        WorkerRole::General => "normal",
        WorkerRole::FixedBlack => "fixed-black",
        WorkerRole::FixedWhite => "fixed-white",
    }
}
struct WorkerDoneGuard(Arc<(Mutex<SharedState>, Condvar)>);

impl Drop for WorkerDoneGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.0.lock() {
            state.running_workers = state.running_workers.saturating_sub(1);
            self.0.1.notify_all();
        }
    }
}

pub fn replay_corpus_results(
    book: &mut SqliteOpeningBook,
    store: &CorpusStore,
    book_hash: &str,
) -> Result<usize, CoordinatorError> {
    let results = store
        .results_requiring_replay(book_hash)
        .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
    let mut added = 0;
    for stored in results {
        if book
            .add_result_if_absent(
                &stored.position_key,
                &crate::search::SearchResult::new(
                    stored.move_usi,
                    stored.response,
                    stored.eval_cp,
                    stored.depth,
                ),
            )
            .map_err(|error| CoordinatorError::Worker(error.to_string()))?
        {
            added += 1;
        }
    }
    Ok(added)
}

fn reserve_normal_work(
    shared: &Arc<(Mutex<SharedState>, Condvar)>,
    options: &NormalRuntimeOptions,
    role: &WorkerRole,
) -> Result<Option<LeafPath>, CoordinatorError> {
    let (state_lock, wake) = &**shared;
    let mut state = state_lock.lock().map_err(|_| CoordinatorError::Poisoned)?;
    loop {
        if state.stop_admission {
            wake.notify_all();
            return Ok(None);
        }
        let limit_reason = if options
            .max_added_positions
            .is_some_and(|limit| state.added_positions >= limit)
        {
            Some("max-added-positions")
        } else if state.searches >= options.max_searches {
            Some("max-searches")
        } else if options
            .max_total_nodes
            .is_some_and(|limit| state.total_nodes.saturating_add(options.nodes) > limit)
        {
            Some("max-total-nodes")
        } else {
            None
        };
        if let Some(reason) = limit_reason {
            state.stop_admission = true;
            if state.stop_reason.is_none() {
                state.stop_reason = Some(reason.to_owned());
            }
            wake.notify_all();
            return Ok(None);
        }
        let SharedState {
            book,
            inflight,
            random,
            ..
        } = &mut *state;
        let book_side = match role {
            WorkerRole::FixedBlack => Some("black"),
            WorkerRole::FixedWhite => Some("white"),
            WorkerRole::General if options.eval_diff.is_some() => Some(options.book_side),
            WorkerRole::General => None,
        };
        let filter = if book_side.is_some()
            || options.eval_diff.is_some()
            || options.min_eval_cp.is_some()
        {
            book.position(&options.root_sfen)
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?
                .and_then(|position| position.entries.iter().map(|entry| entry.eval_cp).max())
                .map(|root_best_eval| PetaFilter {
                    book_side,
                    root_best_eval,
                    eval_diff: options.eval_diff,
                    min_eval_cp: options.min_eval_cp,
                })
        } else {
            None
        };
        let selected = reserve_leaf_path_with_filter_and_random(
            book,
            &options.root_sfen,
            options.multipv,
            options.c_puct,
            options.eval_scale,
            inflight,
            options.max_ply,
            filter.as_ref(),
            Some(random),
        )?;
        match selected {
            Some(path) => {
                let lane = worker_role_label(role).to_owned();
                *state.lane_searches.entry(lane.clone()).or_default() += 1;
                *state.lane_active.entry(lane).or_default() += 1;
                state.searches += 1;
                state.total_nodes += options.nodes;
                state.generation += 1;
                return Ok(Some(path));
            }
            None if state.inflight.is_empty() => {
                state.stop_admission = true;
                wake.notify_all();
                return Ok(None);
            }
            None => {
                state = wake.wait(state).map_err(|_| CoordinatorError::Poisoned)?;
            }
        }
    }
}

fn apply_normal_results(
    state: &mut SharedState,
    path: &LeafPath,
    results: &[crate::search::SearchResult],
) -> Result<(), CoordinatorError> {
    let added_position = state
        .book
        .apply_search_results(path, results)
        .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
    if added_position {
        state.added_positions += 1;
    }
    state.generation += 1;
    Ok(())
}

struct PathCorpusChoice {
    choice: CandidateChoice,
    sfen: String,
    history: Vec<String>,
    path: LeafPath,
    site_rank: usize,
    leaf_distance: usize,
}

fn reserve_corpus_work(
    state: &mut SharedState,
    path: &LeafPath,
    options: &NormalRuntimeOptions,
) -> Result<Option<CorpusWork>, CoordinatorError> {
    let Some(corpus_options) = &options.corpus else {
        return Ok(None);
    };
    if state.corpus_active >= corpus_options.max_concurrent
        || state.searches >= options.max_searches
        || options
            .max_total_nodes
            .is_some_and(|limit| state.total_nodes.saturating_add(corpus_options.nodes) > limit)
    {
        return Ok(None);
    }
    for retry in 0..2 {
        let frontier = state
            .corpus_store
            .as_ref()
            .expect("corpus configured")
            .frontier_state()
            .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
        let width = frontier.progressive_width.max(0) as usize;
        let history: Vec<String> = path
            .steps
            .iter()
            .map(|step| step.move_usi.clone())
            .collect();
        let mut visited: Vec<(String, Vec<String>, LeafPath)> = path
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                (
                    step.sfen.clone(),
                    history[..index].to_vec(),
                    LeafPath {
                        steps: path.steps[..index].to_vec(),
                        leaf_sfen: step.sfen.clone(),
                    },
                )
            })
            .collect();
        visited.push((path.leaf_sfen.clone(), history.clone(), path.clone()));
        let site_order = state
            .site_budget
            .as_ref()
            .expect("corpus configured")
            .order();
        let mut choices = Vec::new();
        let mut truncated = false;
        let mut next_quality_band = None;
        for (visit_index, (sfen, visit_history, visit_path)) in visited.iter().enumerate() {
            let excluded: HashSet<String> = state
                .book
                .move_usis(sfen)
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?
                .into_iter()
                .collect();
            for (site_rank, site_name) in site_order.iter().enumerate() {
                let site = SourceSite::from_name(site_name);
                let probe = state
                    .corpus_store
                    .as_ref()
                    .expect("corpus configured")
                    .probe_position(
                        &corpus_options.snapshot_id,
                        &state.book.position_key(sfen),
                        &excluded,
                        frontier.active_quality_band,
                        site,
                        width,
                        unix_time(),
                    )
                    .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
                truncated |= probe.truncated;
                if let Some(band) = probe.next_quality_band {
                    next_quality_band =
                        Some(next_quality_band.map_or(band, |old: i32| old.min(band)));
                }
                choices.extend(probe.choices.into_iter().map(|choice| PathCorpusChoice {
                    choice,
                    sfen: sfen.clone(),
                    history: visit_history.clone(),
                    path: visit_path.clone(),
                    site_rank,
                    leaf_distance: visited.len() - 1 - visit_index,
                }));
            }
        }
        choices.sort_by(|left, right| {
            left.choice
                .quality_band
                .cmp(&right.choice.quality_band)
                .then_with(|| left.site_rank.cmp(&right.site_rank))
                .then_with(|| right.choice.priority_key.cmp(&left.choice.priority_key))
                .then_with(|| left.leaf_distance.cmp(&right.leaf_distance))
                .then_with(|| left.choice.candidate_id.cmp(&right.choice.candidate_id))
        });
        let mut reservation_collision = false;
        for selected in choices {
            let candidate = state
                .corpus_store
                .as_mut()
                .expect("corpus configured")
                .reserve_choice(
                    &selected.choice,
                    &corpus_options.snapshot_id,
                    300.0,
                    unix_time(),
                )
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
            let Some(candidate) = candidate else {
                reservation_collision = true;
                continue;
            };
            state
                .corpus_store
                .as_mut()
                .expect("corpus configured")
                .record_rollout_observation(
                    &RolloutObservation {
                        eligible_miss: false,
                        reserved: true,
                        truncated_in_active_band: truncated,
                        smallest_higher_band: next_quality_band,
                    },
                    corpus_options.saturation_window,
                    unix_time(),
                )
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
            state.last_candidate = Some(LastCandidateStatus {
                candidate_id: selected.choice.candidate_id,
                position_key: selected.choice.position_key.clone(),
                move_usi: selected.choice.move_usi.clone(),
                source: selected.choice.source.clone(),
                quality_band: selected.choice.quality_band,
                result: "reserved".to_owned(),
            });
            eprintln!(
                "[corpus_reserve] depth={} position_key={:?} move={} source={} band={} n={} task_id={}",
                selected.history.len(),
                selected.choice.position_key,
                selected.choice.move_usi,
                selected.choice.source,
                selected.choice.quality_band,
                frontier.progressive_width,
                candidate.id
            );
            state.corpus_active += 1;
            state.searches += 1;
            state.total_nodes += corpus_options.nodes;
            let site = source_site_name(selected.choice.source_site);
            state
                .site_budget
                .as_mut()
                .expect("corpus configured")
                .record(site, corpus_options.nodes as i64)
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
            state
                .corpus_store
                .as_mut()
                .expect("corpus configured")
                .add_metric(&format!("corpus_nodes:{site}"), corpus_options.nodes as i64)
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
            return Ok(Some(CorpusWork {
                candidate,
                sfen: state.book.output_sfen(&selected.sfen),
                history: selected.history,
                path: selected.path,
            }));
        }
        let transition = state
            .corpus_store
            .as_mut()
            .expect("corpus configured")
            .record_rollout_observation(
                &RolloutObservation {
                    eligible_miss: !reservation_collision,
                    reserved: false,
                    truncated_in_active_band: truncated,
                    smallest_higher_band: next_quality_band,
                },
                corpus_options.saturation_window,
                unix_time(),
            )
            .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
        if transition != FrontierTransition::None {
            let updated = state
                .corpus_store
                .as_ref()
                .expect("corpus configured")
                .frontier_state()
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
            let reason = match transition {
                FrontierTransition::Width { .. } => "active_band_truncated",
                FrontierTransition::Band { .. } => "next_quality_band",
                FrontierTransition::None => unreachable!(),
            };
            eprintln!(
                "[frontier_change] band={} n={} reason={} eligible_miss_count={}",
                updated.active_quality_band,
                updated.progressive_width,
                reason,
                updated.eligible_miss_count
            );
        }
        if retry == 0 && transition != FrontierTransition::None {
            continue;
        }
        return Ok(None);
    }
    Ok(None)
}

fn source_site_name(site: SourceSite) -> &'static str {
    match site {
        SourceSite::Wcsc => "wcsc",
        SourceSite::Denryu => "denryu",
        SourceSite::Floodgate => "floodgate",
        SourceSite::Unknown => "unknown",
    }
}
fn apply_corpus_result(
    state: &mut SharedState,
    work: &CorpusWork,
    result: Result<crate::search::SearchResult, EngineError>,
    options: &NormalRuntimeOptions,
    elapsed_ms: u128,
) -> Result<(), CoordinatorError> {
    let corpus_options = options.corpus.as_ref().expect("work requires corpus");
    if state.discard_results {
        state
            .corpus_store
            .as_mut()
            .expect("corpus configured")
            .interrupt_search(work.candidate.id, unix_time())
            .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
        return Ok(());
    }
    match result {
        Ok(result) => {
            let added = state
                .book
                .add_result_if_absent(&work.sfen, &result)
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
            state
                .book
                .apply_search_results(&work.path, std::slice::from_ref(&result))
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
            state
                .corpus_store
                .as_mut()
                .expect("corpus configured")
                .complete_search(
                    work.candidate.id,
                    &SearchCompletion {
                        eval_cp: result.eval_cp,
                        response: result.response,
                        depth: result.depth,
                        nodes: corpus_options.nodes as i64,
                        engine_config_id: corpus_options.engine_fingerprint.clone(),
                        now: unix_time(),
                    },
                )
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
            if added {
                state.corpus_add_successes += 1;
            }
            eprintln!(
                "[corpus_complete] depth={} position_key={:?} move={} source={} result={} fingerprint={} elapsed_ms={}",
                work.history.len(),
                work.candidate.position_key,
                work.candidate.move_usi,
                work.candidate.source,
                if added { "added" } else { "already-present" },
                corpus_options.engine_fingerprint,
                elapsed_ms
            );
            if let Some(last) = state.last_candidate.as_mut()
                && last.candidate_id == work.candidate.id
            {
                last.result = if added { "added" } else { "already-present" }.to_owned();
            }
            state.generation += 1;
        }
        Err(error) => {
            if let Some(last) = state.last_candidate.as_mut()
                && last.candidate_id == work.candidate.id
            {
                last.result = format!("failed:{error}");
            }
            let status = state
                .corpus_store
                .as_mut()
                .expect("corpus configured")
                .fail_engine_search(
                    work.candidate.id,
                    &error.to_string(),
                    &corpus_options.engine_fingerprint,
                    unix_time(),
                )
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?;
            let failure_class = if status == SearchTaskStatus::PermanentFailed {
                "engine_retry_exhausted"
            } else {
                "engine_transient"
            };
            eprintln!(
                "[corpus_failure] depth={} position_key={:?} move={} source={} failure_class={} fingerprint={} elapsed_ms={} error={:?}",
                work.history.len(),
                work.candidate.position_key,
                work.candidate.move_usi,
                work.candidate.source,
                failure_class,
                corpus_options.engine_fingerprint,
                elapsed_ms,
                error.to_string()
            );
        }
    }
    Ok(())
}

fn runtime_status_snapshot(
    state: &SharedState,
    engine_fingerprint: &str,
) -> Result<RuntimeStatusSnapshot, CoordinatorError> {
    let (frontier, tasks, site_nodes, revision) = if let Some(store) = state.corpus_store.as_ref() {
        (
            store
                .frontier_state()
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?,
            store
                .task_status_counts()
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?,
            store
                .site_node_metrics()
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?,
            store
                .corpus_revision()
                .map_err(|error| CoordinatorError::Worker(error.to_string()))?,
        )
    } else {
        (
            crate::corpus::FrontierState {
                active_quality_band: 0,
                progressive_width: 1,
                eligible_miss_count: 0,
                truncated_candidate_seen: false,
                next_quality_band_seen: None,
            },
            TaskStatusCounts::default(),
            std::collections::BTreeMap::new(),
            0,
        )
    };
    Ok(RuntimeStatusSnapshot {
        run_id: state.run_id.clone(),
        pid: std::process::id(),
        started_at: state.started_at,
        updated_at: unix_time(),
        searches: state.searches,
        added_positions: state.added_positions,
        total_nodes: state.total_nodes,
        running_workers: state.running_workers,
        corpus_active: state.corpus_active,
        lane_searches: state.lane_searches.clone(),
        lane_active: state.lane_active.clone(),
        last_search: state.last_search.clone(),
        active_quality_band: frontier.active_quality_band,
        progressive_width: frontier.progressive_width,
        eligible_miss_count: frontier.eligible_miss_count,
        tasks,
        book_add_successes: state.corpus_add_successes,
        site_nodes,
        last_candidate: state.last_candidate.clone(),
        corpus_revision: revision,
        priority_policy_version: crate::priority::POLICY_VERSION.to_owned(),
        engine_fingerprint: engine_fingerprint.to_owned(),
        last_book_save: state.last_book_save.clone(),
    })
}
fn position_root(root_sfen: &str) -> PositionRoot {
    if root_sfen == crate::STARTPOS_SFEN {
        PositionRoot::Startpos
    } else {
        PositionRoot::Sfen(root_sfen.to_owned())
    }
}

fn default_random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64)
        ^ u64::from(std::process::id())
}
fn unix_time() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
