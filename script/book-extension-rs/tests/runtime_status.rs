use std::collections::BTreeMap;

use book_extension_runtime::runtime_status::{
    RuntimeStatusSnapshot, TaskStatusCounts, write_runtime_status_atomic,
};

#[test]
fn writes_complete_status_atomically_without_leaving_temp_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime-status.json");
    let snapshot = RuntimeStatusSnapshot {
        updated_at: 1.0,
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
    second.updated_at = 2.0;
    write_runtime_status_atomic(&path, &second).unwrap();

    let actual: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(actual["updated_at"], 2.0);
    let siblings: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(siblings, ["runtime-status.json"]);
}
