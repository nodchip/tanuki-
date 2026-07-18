use std::collections::BTreeMap;

use book_extension_runtime::runtime_status::{
    LastSearchStatus, RuntimeStatusSnapshot, TaskStatusCounts, write_runtime_status_atomic,
};

#[test]
fn writes_complete_status_atomically_without_leaving_temp_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime-status.json");
    let snapshot = RuntimeStatusSnapshot {
        run_id: "run-123".to_owned(),
        pid: 42,
        started_at: 100.0,
        updated_at: 101.0,
        searches: 9,
        added_positions: 4,
        total_nodes: 900,
        running_workers: 3,
        corpus_active: 1,
        lane_searches: BTreeMap::from([
            ("normal".to_owned(), 5),
            ("vulnerability-black".to_owned(), 4),
        ]),
        lane_active: BTreeMap::from([
            ("normal".to_owned(), 2),
            ("vulnerability-black".to_owned(), 1),
        ]),
        last_search: Some(LastSearchStatus {
            lane: "normal".to_owned(),
            depth: 83,
            position_key: "position".to_owned(),
            status: "ok".to_owned(),
            elapsed_ms: 1234,
        }),
        active_quality_band: 0,
        progressive_width: 1,
        eligible_miss_count: 2,
        tasks: TaskStatusCounts::default(),
        book_add_successes: 3,
        site_nodes: BTreeMap::new(),
        last_candidate: None,
        corpus_revision: 1,
        priority_policy_version: "corpus-priority-v2".to_owned(),
        engine_fingerprint: "fingerprint".to_owned(),
        last_book_save: None,
    };
    write_runtime_status_atomic(&path, &snapshot).unwrap();
    let mut second = snapshot.clone();
    second.updated_at = 102.0;
    write_runtime_status_atomic(&path, &second).unwrap();

    let actual: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(actual["run_id"], "run-123");
    assert_eq!(actual["updated_at"], 102.0);
    assert_eq!(actual["searches"], 9);
    assert_eq!(actual["lane_active"]["normal"], 2);
    assert_eq!(actual["last_search"]["depth"], 83);
    let siblings: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(siblings, ["runtime-status.json"]);
}
