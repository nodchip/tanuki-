use std::{fs, process::Command};

use tempfile::TempDir;

#[test]
fn probe_emits_deterministic_normalized_json_without_python() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("game.csa");
    fs::write(
        &input,
        "V2.2\nN+Black\nN-White\nPI\n+\n+7776FU\n-3334FU\n%TORYO\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_corpus-record-probe"))
        .args(["--input", input.to_str().unwrap(), "--format", "csa"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "accepted");
    assert_eq!(
        value["games"][0]["players"],
        serde_json::json!(["Black", "White"])
    );
    assert_eq!(
        value["games"][0]["moves"],
        serde_json::json!(["7g7f", "3c3d"])
    );
    assert_eq!(value["games"][0]["positions"].as_array().unwrap().len(), 2);
}

#[test]
fn probe_returns_structured_error_and_nonzero_status() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("bad.csa");
    fs::write(&input, "V2.2\nPI\n+\nT0\n+7776FU\n%TORYO\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_corpus-record-probe"))
        .args(["--input", input.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "excluded");
    assert_eq!(value["error"]["kind"], "TimeBeforeFirstMove");
    assert_eq!(value["error"]["line"], 4);
}
