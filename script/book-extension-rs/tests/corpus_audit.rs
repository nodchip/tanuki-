use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

#[test]
fn audits_records_with_progress_and_reason_counts() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("accepted.csa"),
        b"V2.2\nN+Black\nN-White\nPI\n+\n+7776FU\n-3334FU\n%TORYO\n",
    )
    .unwrap();
    let error_log = dir.path().join("errors.jsonl");
    let game_log = dir.path().join("games.jsonl");
    std::fs::write(
        dir.path().join("excluded.csa"),
        b"V2.2\nPI\n+\nT0\n+7776FU\n%TORYO\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_corpus-audit"))
        .args([
            "--source",
            dir.path().to_str().unwrap(),
            "--progress-every",
            "1",
            "--error-log",
            error_log.to_str().unwrap(),
            "--game-log",
            game_log.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["records"], 2);
    assert_eq!(summary["accepted_games"], 1);
    assert_eq!(summary["excluded_records"], 1);
    assert_eq!(summary["moves"], 2);
    assert_eq!(summary["reasons"]["TimeBeforeFirstMove"], 1);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("[corpus-audit] event=progress records=1"));
    assert!(stderr.contains("[corpus-audit] event=progress records=2"));
    let errors = std::fs::read_to_string(error_log).unwrap();
    let error: Value = serde_json::from_str(errors.trim()).unwrap();
    assert_eq!(error["kind"], "TimeBeforeFirstMove");
    assert_eq!(error["source_path"], "excluded.csa");
    assert_eq!(error["line"], 4);
    let games = std::fs::read_to_string(game_log).unwrap();
    let game: Value = serde_json::from_str(games.trim()).unwrap();
    assert_eq!(game["source_path"], "accepted.csa");
    assert_eq!(
        game["sha256"],
        "c0ff1d317d0d7038f6539dded80fb91eb76a5476bc249620f94d46b5e2f30a1f"
    );
    assert_eq!(game["move_count"], 2);
    assert_eq!(game["position_count"], 2);
    assert_eq!(game["endgame"], "%TORYO");
    assert_eq!(game["players"], serde_json::json!(["Black", "White"]));
    assert_eq!(game["moves_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(game["positions_sha256"].as_str().unwrap().len(), 64);
}
