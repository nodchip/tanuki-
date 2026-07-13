use std::{fs, time::UNIX_EPOCH};

use book_extension_runtime::runtime::{BookFileLock, HeartbeatMonitor, RunStats, StopLimits};

#[test]
fn heartbeat_detects_staleness_and_manual_stop() {
    let directory = tempfile::tempdir().unwrap();
    let heartbeat = directory.path().join("heartbeat.json");
    let stop = directory.path().join("graceful-stop.request");
    fs::write(&heartbeat, "{}").unwrap();
    let modified = fs::metadata(&heartbeat)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let monitor = HeartbeatMonitor::new(&heartbeat, 10.0, modified, Some(stop.clone())).unwrap();
    assert_eq!(monitor.stop_reason(modified + 9.9), None);
    assert_eq!(
        monitor.stop_reason(modified + 10.1),
        Some("jenkins-heartbeat-expired")
    );
    fs::write(stop, "").unwrap();
    assert_eq!(
        monitor.stop_reason(modified + 1.0),
        Some("manual-stop-request")
    );
}

#[test]
fn missing_heartbeat_uses_startup_grace() {
    let directory = tempfile::tempdir().unwrap();
    let monitor =
        HeartbeatMonitor::new(&directory.path().join("missing.json"), 10.0, 100.0, None).unwrap();
    assert!(!monitor.expired(109.9));
    assert!(monitor.expired(110.1));
}

#[test]
fn stop_limits_match_python_precedence_and_node_admission() {
    let limits = StopLimits {
        max_added_positions: Some(2),
        max_searches: Some(3),
        max_total_nodes: Some(400),
        max_runtime_sec: Some(10.0),
    };
    assert_eq!(limits.stop_reason(&RunStats::new(100.0), 101.0), None);
    let mut stats = RunStats::new(100.0);
    stats.searches = 3;
    stats.total_nodes = 400;
    assert_eq!(limits.stop_reason(&stats, 101.0), Some("max-searches"));
    assert!(!limits.can_start_search(&stats, 1));
}

#[test]
fn book_lock_rejects_second_owner_and_releases_on_drop() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("working.db.lock");
    let first = BookFileLock::acquire(&path, r#"{"run_id":"first"}"#).unwrap();
    assert!(BookFileLock::acquire(&path, r#"{"run_id":"second"}"#).is_err());
    assert!(fs::read_to_string(&path).unwrap().contains("first"));
    drop(first);
    let second = BookFileLock::acquire(&path, r#"{"run_id":"second"}"#).unwrap();
    drop(second);
}
