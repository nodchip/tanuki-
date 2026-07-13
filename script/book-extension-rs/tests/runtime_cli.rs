use std::process::Command;

use book_extension_runtime::{corpus::CorpusStore, storage::sha256_file};

#[test]
fn rust_runtime_extends_and_saves_one_normal_leaf_without_python() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    std::fs::write(
        &input,
        "#YANEURAOU-DB2016 1.00\nsfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1\n",
    )
    .unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 1
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 5
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("stop.request"), "stale").unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "100",
            "--multipv",
            "2",
            "--max-searches",
            "1",
        ])
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let saved = std::fs::read_to_string(&output).unwrap();
    assert!(saved.contains("7g7f 3c3d 25 10 0"), "{saved}");
    assert!(saved.contains("2g2f 8c8d 10 9 0"), "{saved}");
}

#[test]
fn normal_lane_watchdog_restarts_and_retries_three_times() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    std::fs::write(
        &input,
        format!(
            "#YANEURAOU-DB2016 1.00\nsfen {}\n",
            book_extension_runtime::STARTPOS_SFEN
        ),
    )
    .unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 1
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 0.1
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "997",
            "--multipv",
            "1",
            "--max-searches",
            "1",
            "--usi-search-timeout-sec",
            "0.1",
        ])
        .output()
        .unwrap();

    assert!(!result.status.success());
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert_eq!(stderr.matches("[engine-retry]").count(), 2, "{stderr}");
    assert!(stderr.contains("USI search timed out"), "{stderr}");
}
#[test]
fn rust_runtime_starts_configured_worker_count_and_runs_two_searches() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    std::fs::write(
        &input,
        "#YANEURAOU-DB2016 1.00\nsfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1\n",
    ).unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 2
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 2
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 5
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "100",
            "--multipv",
            "2",
            "--max-searches",
            "2",
        ])
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stderr.contains("[runtime] engines=2"), "{stderr}");
    let saved = std::fs::read_to_string(&output).unwrap();
    assert!(saved.contains("3c3d 2g2f 25 10 0"), "{saved}");
}

#[test]
fn two_workers_search_in_parallel_without_holding_global_state_lock() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    std::fs::write(
        &input,
        format!(
            "#YANEURAOU-DB2016 1.00\nsfen {}\n7g7f 3c3d 100 1 0\n2g2f 8c8d 10 1 0\n",
            book_extension_runtime::STARTPOS_SFEN
        ),
    )
    .unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 2
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 2
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 5
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();

    let started = std::time::Instant::now();
    let result = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "995",
            "--multipv",
            "2",
            "--max-searches",
            "2",
        ])
        .output()
        .unwrap();
    let elapsed = started.elapsed();

    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        elapsed < std::time::Duration::from_millis(850),
        "two 500 ms searches serialized: elapsed={elapsed:?}, stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&result.stderr)
            .matches("[save]")
            .count(),
        1,
        "result notifications must not trigger per-result saves"
    );
}
#[test]
fn configured_vulnerability_black_worker_forces_target_book_move() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let target = directory.path().join("target.db");
    let output = directory.path().join("output.db");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    let root = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
    std::fs::write(&input, format!("#YANEURAOU-DB2016 1.00\nsfen {root}\n")).unwrap();
    std::fs::write(
        &target,
        format!("#YANEURAOU-DB2016 1.00\nsfen {root}\n7g7f 3c3d 10 1 0\n2g2f 8c8d 100 1 0\n"),
    )
    .unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
vulnerability_black = 1
vulnerability_white = 0
general = 0
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 5
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "100",
            "--multipv",
            "2",
            "--max-searches",
            "1",
            "--black-target",
            target.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let saved = std::fs::read_to_string(&output).unwrap();
    let root_section = saved.split("sfen ").nth(1).unwrap();
    assert!(root_section.contains("2g2f 8c8d -25 1 1"), "{saved}");
    assert!(!root_section.contains("7g7f 3c3d 10 1 0"), "{saved}");
}

#[test]
fn corpus_lane_adds_unregistered_move_from_visited_position_with_searchmoves() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let corpus = directory.path().join("corpus.sqlite");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    let root = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
    std::fs::write(&input, format!("#YANEURAOU-DB2016 1.00\nsfen {root}\n")).unwrap();
    let mut store = CorpusStore::open(&corpus).unwrap();
    store
        .upsert_candidate(root, "8g8f", &[], "wcsc:event", "999")
        .unwrap();
    drop(store);
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 1
[corpus]
enabled = true
max_concurrent_searches = 1
general_pool_node_share = 0.25
saturation_window = 100
wcsc_weight = 40
denryu_weight = 40
floodgate_weight = 20
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 5
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "100",
            "--multipv",
            "2",
            "--max-searches",
            "2",
            "--corpus-db",
            corpus.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let saved = std::fs::read_to_string(&output).unwrap();
    assert!(saved.contains("8g8f 3c3d 77 11 0"), "{saved}");
    let store = CorpusStore::open(&corpus).unwrap();
    assert!(store.unpersisted_results().unwrap().is_empty());
    assert!(
        store
            .has_checkpoint(&sha256_file(&output).unwrap())
            .unwrap()
    );
    assert_eq!(store.metric("corpus_nodes:wcsc").unwrap(), 33);
}

#[test]
fn manual_stop_interrupts_active_search_and_discards_its_result() {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let stop = directory.path().join("stop.request");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    let root = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
    std::fs::write(&input, format!("#YANEURAOU-DB2016 1.00\nsfen {root}\n")).unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 1
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 5
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "999",
            "--multipv",
            "1",
            "--max-searches",
            "1",
            "--stop-request-path",
            stop.to_str().unwrap(),
        ])
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(150));
    std::fs::write(&stop, "manual").unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    if child.try_wait().unwrap().is_none() {
        child.kill().unwrap();
        panic!("Rust runtime did not stop after manual stop request");
    }
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let saved = std::fs::read_to_string(output).unwrap();
    assert!(
        !saved.contains("7g7f"),
        "interrupted result leaked: {saved}"
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("manual-stop-request"),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn periodic_save_runs_during_search_and_final_save_keeps_previous_generation() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    let root = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
    std::fs::write(&input, format!("#YANEURAOU-DB2016 1.00\nsfen {root}\n")).unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 1
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 0.05
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 5
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "998",
            "--multipv",
            "2",
            "--max-searches",
            "1",
        ])
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(std::fs::read_to_string(&output).unwrap().contains("7g7f"));
    let backup = output.with_file_name(format!(
        "{}.001.bak",
        output.file_name().unwrap().to_string_lossy()
    ));
    assert!(
        backup.exists(),
        "periodic generation backup missing; stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!std::fs::read_to_string(backup).unwrap().contains("7g7f"));
}

#[test]
fn max_total_nodes_stops_before_starting_a_search_that_exceeds_budget() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    let root = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
    std::fs::write(&input, format!("#YANEURAOU-DB2016 1.00\nsfen {root}\n")).unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 1
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 5
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "100",
            "--multipv",
            "2",
            "--max-searches",
            "10",
            "--max-total-nodes",
            "100",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stderr.contains("reason=max-total-nodes"), "{stderr}");
    assert!(std::fs::read_to_string(output).unwrap().contains("7g7f"));
}

#[test]
fn max_runtime_interrupts_active_search_and_reports_reason() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    let root = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
    std::fs::write(&input, format!("#YANEURAOU-DB2016 1.00\nsfen {root}\n")).unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 1
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 5
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "999",
            "--multipv",
            "1",
            "--max-searches",
            "10",
            "--max-runtime-sec",
            "0.1",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stderr.contains("reason=max-runtime-sec"), "{stderr}");
    assert!(!std::fs::read_to_string(output).unwrap().contains("7g7f"));
}

#[test]
fn stale_heartbeat_interrupts_active_search_and_discards_result() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let heartbeat = directory.path().join("jenkins.heartbeat");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    let root = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
    std::fs::write(&input, format!("#YANEURAOU-DB2016 1.00\nsfen {root}\n")).unwrap();
    std::fs::write(&heartbeat, "initial").unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 1
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 0.1
usi_stop_timeout_sec = 5
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "999",
            "--multipv",
            "1",
            "--max-searches",
            "10",
            "--heartbeat-path",
            heartbeat.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(
        stderr.contains("reason=jenkins-heartbeat-expired"),
        "{stderr}"
    );
    assert!(!std::fs::read_to_string(output).unwrap().contains("7g7f"));
}
#[test]
fn unresponsive_engine_is_killed_after_usi_stop_timeout() {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let state = directory.path().join("state");
    let config = directory.path().join("config.toml");
    let root = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
    std::fs::write(&input, format!("#YANEURAOU-DB2016 1.00\nsfen {root}\n")).unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 1
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "{}"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 0.1
"#,
            state.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_book-extender"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--engine",
            env!("CARGO_BIN_EXE_fake-usi-engine"),
            "--nodes",
            "997",
            "--multipv",
            "1",
            "--max-searches",
            "10",
            "--max-runtime-sec",
            "0.1",
        ])
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    if child.try_wait().unwrap().is_none() {
        child.kill().unwrap();
        panic!("unresponsive USI engine prevented Rust runtime shutdown");
    }
    let result = child.wait_with_output().unwrap();
    assert!(
        !result.status.success(),
        "forced engine EOF must be reported"
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("USI engine failed:"),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
}
