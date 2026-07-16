use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader, Cursor, Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
};

use book_extension_runtime::{STARTPOS_SFEN, corpus::CorpusStore};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

fn zip_fixture() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut bytes);
        zip.start_file("game.csa", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"V2.2\nN+Alpha\nN-Beta\nPI\n+\n+7776FU\n-3334FU\n%TORYO\n")
            .unwrap();
        zip.finish().unwrap();
    }
    bytes.into_inner()
}

fn serve_once(payload: Vec<u8>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            payload.len()
        )
        .unwrap();
        stream.write_all(&payload).unwrap();
    });
    (format!("http://{address}/records.zip"), handle)
}

#[test]
fn one_command_builds_validated_and_published_corpus() {
    let dir = TempDir::new().unwrap();
    let archive = zip_fixture();
    let (url, server) = serve_once(archive.clone());
    let manifest = json!({
        "manifest_version": 1,
        "created_at_utc": "2026-07-15T00:00:00Z",
        "sources": [{
            "site":"wcsc","event":"fixture","year":2026,"retrieved_at":1000,
            "url":url,"relative_path":"records.zip","size":archive.len(),
            "sha256":format!("{:x}", Sha256::digest(&archive))
        }]
    });
    std::fs::write(
        dir.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let profile = json!({
        "schema_version":1,"name":"fixture","priority_reference_year":2026,"manifest":"manifest.json",
        "ingests":[{"site":"wcsc","event":"fixture","year":2026,"retrieved_at":1000,"inputs":["records.zip"]}],
        "ranking_files":[],"alias_files":[],"rating_file":null,"rating_config":null,"input_book":null,
        "coverage_snapshot_id":"fixture-initial"
    });
    std::fs::write(
        dir.path().join("profile.json"),
        serde_json::to_vec(&profile).unwrap(),
    )
    .unwrap();
    let book = dir.path().join("book.db");
    std::fs::write(
        &book,
        "#YANEURAOU-DB2016 1.00\nsfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1\n7g7f none 0 0 0\n",
    )
    .unwrap();
    let state = dir.path().join("state");

    let output = Command::new(env!("CARGO_BIN_EXE_corpus-builder"))
        .args(["build", "--profile"])
        .arg(dir.path().join("profile.json"))
        .arg("--state-dir")
        .arg(&state)
        .arg("--input-book")
        .arg(&book)
        .output()
        .unwrap();
    server.join().unwrap();

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(state.join("corpus.sqlite").is_file());
    assert!(state.join("coverage-initial.json").is_file());
    assert!(state.join("snapshot.json").is_file());
    assert!(state.join("corpus-build-summary.json").is_file());
    assert!(!state.join("corpus.sqlite-wal").exists());
    assert!(!state.join("corpus.sqlite-shm").exists());
    let connection = rusqlite::Connection::open(state.join("corpus.sqlite")).unwrap();
    let quality_meta: (String, String, i64) = connection
        .query_row(
            "SELECT
               (SELECT value FROM meta WHERE key='priority_reference_year'),
               (SELECT value FROM meta WHERE key='priority_policy_version'),
               (SELECT COUNT(*) FROM candidate WHERE quality_band<0 OR source_site NOT BETWEEN 0 AND 3
                    OR recent_occurrences<0 OR occurrences<0 OR recent_occurrences>occurrences)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(quality_meta.0, "2026");
    assert_eq!(quality_meta.1, "corpus-priority-v2");
    assert_eq!(quality_meta.2, 0);
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM logical_game", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let summary: serde_json::Value =
        serde_json::from_reader(File::open(state.join("corpus-build-summary.json")).unwrap())
            .unwrap();
    assert_eq!(summary["status"], "success");
    assert_eq!(summary["accepted"], 1);
    assert_eq!(summary["record_members"], 1);
    assert_eq!(summary["filtered_members"], 0);
    assert_eq!(summary["unmatched_participants"]["Alpha"], 1);
    assert_eq!(summary["unmatched_participants"]["Beta"], 1);
    assert!(summary["moves_per_second"].as_f64().unwrap() > 0.0);
    assert!(summary["positions_per_second"].as_f64().unwrap() > 0.0);
    assert!(summary["sqlite_rows_per_second"].as_f64().unwrap() > 0.0);
    assert!(String::from_utf8_lossy(&output.stderr).contains("phase=ingest"));
    drop(connection);

    let first_database = std::fs::read(state.join("corpus.sqlite")).unwrap();
    let second = Command::new(env!("CARGO_BIN_EXE_corpus-builder"))
        .args(["build", "--profile"])
        .arg(dir.path().join("profile.json"))
        .arg("--state-dir")
        .arg(&state)
        .arg("--input-book")
        .arg(&book)
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        std::fs::read(state.join("corpus.sqlite.previous")).unwrap(),
        first_database
    );

    let mut runtime_store = CorpusStore::open(&state.join("corpus.sqlite")).unwrap();
    let candidate = runtime_store
        .reserve_for_position(
            "fixture-book",
            STARTPOS_SFEN,
            &HashSet::new(),
            1,
            60.0,
            None,
            2_000.0,
        )
        .unwrap()
        .expect("Rust runtime reserves a candidate from the generated corpus");
    assert_eq!(candidate.move_usi, "7g7f");
    assert!(candidate.history.is_empty());
}

#[test]
fn stop_request_during_download_preserves_active_and_writes_run_summary() {
    let dir = TempDir::new().unwrap();
    let archive = zip_fixture();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let state = dir.path().join("state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("corpus.sqlite"), b"existing-active").unwrap();
    let stop_path = state.join("stop.request");
    let stop_for_server = stop_path.clone();
    let payload = archive.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).unwrap();
        std::fs::write(stop_for_server, "jenkins-abort").unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            payload.len()
        )
        .unwrap();
        stream.write_all(&payload).unwrap();
    });
    let manifest = json!({
        "manifest_version": 1,
        "created_at_utc": "2026-07-15T00:00:00Z",
        "sources": [{
            "site":"wcsc","event":"fixture","year":2026,"retrieved_at":1000,
            "url":format!("http://{address}/records.zip"),"relative_path":"records.zip",
            "size":archive.len(),"sha256":format!("{:x}", Sha256::digest(&archive))
        }]
    });
    std::fs::write(
        dir.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let profile = json!({
        "schema_version":1,"name":"fixture","priority_reference_year":2026,"manifest":"manifest.json",
        "ingests":[{"site":"wcsc","event":"fixture","year":2026,"retrieved_at":1000,"inputs":["records.zip"]}],
        "ranking_files":[],"alias_files":[],"rating_file":null,"rating_config":null,"input_book":null,
        "coverage_snapshot_id":"fixture-initial"
    });
    std::fs::write(
        dir.path().join("profile.json"),
        serde_json::to_vec(&profile).unwrap(),
    )
    .unwrap();
    let book = dir.path().join("book.db");
    std::fs::write(&book, "#YANEURAOU-DB2016 1.00\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_corpus-builder"))
        .args(["build", "--profile"])
        .arg(dir.path().join("profile.json"))
        .arg("--state-dir")
        .arg(&state)
        .arg("--input-book")
        .arg(&book)
        .output()
        .unwrap();
    server.join().unwrap();

    assert_eq!(output.status.code(), Some(130));
    assert!(!state.join("downloads/fixture/records.zip").exists());
    assert_eq!(
        std::fs::read(state.join("corpus.sqlite")).unwrap(),
        b"existing-active"
    );
    let summaries: Vec<_> = std::fs::read_dir(state.join("build"))
        .unwrap()
        .map(|entry| entry.unwrap().path().join("corpus-build-summary.json"))
        .filter(|path| path.is_file())
        .collect();
    assert_eq!(summaries.len(), 1);
    let summary: serde_json::Value =
        serde_json::from_reader(File::open(&summaries[0]).unwrap()).unwrap();
    assert_eq!(summary["status"], "stopped");
    assert_eq!(summary["phase"], "download");
    assert_eq!(summary["exit_code"], 130);
}
fn large_zip_fixture(count: usize) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut bytes);
        for index in 0..count {
            zip.start_file(format!("game-{index:05}.csa"), SimpleFileOptions::default())
                .unwrap();
            write!(
                zip,
                "V2.2\nN+Alpha-{index}\nN-Beta\nPI\n+\n+7776FU\n-3334FU\n%TORYO\n"
            )
            .unwrap();
        }
        zip.finish().unwrap();
    }
    bytes.into_inner()
}

#[test]
fn stopped_ingest_resumes_from_the_last_committed_batch() {
    const RECORDS: usize = 1_500;
    let dir = TempDir::new().unwrap();
    let archive = large_zip_fixture(RECORDS);
    let (url, server) = serve_once(archive.clone());
    let manifest = json!({
        "manifest_version": 1,
        "created_at_utc": "2026-07-15T00:00:00Z",
        "sources": [{
            "site":"wcsc","event":"resume-fixture","year":2026,"retrieved_at":1000,
            "url":url,"relative_path":"records.zip","size":archive.len(),
            "sha256":format!("{:x}", Sha256::digest(&archive))
        }]
    });
    std::fs::write(
        dir.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let profile = json!({
        "schema_version":1,"name":"resume-fixture","priority_reference_year":2026,"manifest":"manifest.json",
        "ingests":[{"site":"wcsc","event":"resume-fixture","year":2026,"retrieved_at":1000,"inputs":["records.zip"]}],
        "ranking_files":[],"alias_files":[],"rating_file":null,"rating_config":null,"input_book":null,
        "coverage_snapshot_id":"fixture-initial"
    });
    std::fs::write(
        dir.path().join("profile.json"),
        serde_json::to_vec(&profile).unwrap(),
    )
    .unwrap();
    let book = dir.path().join("book.db");
    std::fs::write(&book, "#YANEURAOU-DB2016 1.00\n").unwrap();
    let state = dir.path().join("state");

    let mut child = Command::new(env!("CARGO_BIN_EXE_corpus-builder"))
        .args(["build", "--profile"])
        .arg(dir.path().join("profile.json"))
        .arg("--state-dir")
        .arg(&state)
        .arg("--input-book")
        .arg(&book)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let mut requested = false;
    for line in BufReader::new(stderr).lines() {
        let line = line.unwrap();
        if !requested && line.contains("event=batch_done") {
            std::fs::write(state.join("stop.request"), "test-stop").unwrap();
            requested = true;
        }
    }
    let status = child.wait().unwrap();
    server.join().unwrap();
    assert!(requested, "first run never committed a batch");
    assert_eq!(status.code(), Some(130));
    assert!(!state.join("corpus.sqlite").exists());

    let output = Command::new(env!("CARGO_BIN_EXE_corpus-builder"))
        .args(["build", "--profile"])
        .arg(dir.path().join("profile.json"))
        .arg("--state-dir")
        .arg(&state)
        .arg("--input-book")
        .arg(&book)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_reader(File::open(state.join("corpus-build-summary.json")).unwrap())
            .unwrap();
    assert_eq!(summary["resumed"], true);
    assert!(summary["resumed_records"].as_u64().unwrap() >= 500);
    assert_eq!(summary["accepted"], RECORDS);
    let connection = rusqlite::Connection::open(state.join("corpus.sqlite")).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM source_game", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        RECORDS as i64
    );
}
