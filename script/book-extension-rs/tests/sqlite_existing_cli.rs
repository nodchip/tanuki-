use std::{
    path::Path,
    process::{Command, Output},
};

use book_extension_runtime::sqlite_book::SqliteOpeningBook;

fn write_config(directory: &Path) -> std::path::PathBuf {
    let state = directory.join("state");
    let config = directory.join("config.toml");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        &config,
        format!(
            r#"[workers]
engine_count = 1
threads_per_engine = 1
fixed_black = 0
fixed_white = 0
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
    config
}

fn run_without_input(config: &Path, database: Option<&Path>, output: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_book-extender"));
    command
        .arg("--config")
        .arg(config)
        .arg("--output")
        .arg(output)
        .arg("--engine")
        .arg(env!("CARGO_BIN_EXE_fake-usi-engine"))
        .args(["--nodes", "100", "--multipv", "2", "--max-searches", "1"]);
    if let Some(database) = database {
        command.arg("--database").arg(database);
    }
    command.output().unwrap()
}

#[test]
fn starts_from_a_nonempty_existing_sqlite_book_without_input() {
    let directory = tempfile::tempdir().unwrap();
    let config = write_config(directory.path());
    let database = directory.path().join("opening-book.sqlite");
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    std::fs::write(
        &input,
        format!(
            "#YANEURAOU-DB2016 1.00\nsfen {}\n",
            book_extension_runtime::STARTPOS_SFEN
        ),
    )
    .unwrap();
    let mut book = SqliteOpeningBook::open(&database, false).unwrap();
    book.import_yaneuraou(&input).unwrap();
    drop(book);

    let result = run_without_input(&config, Some(&database), &output);

    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("[book_open] database="), "{stderr}");
    assert!(!stderr.contains("[book_import]"), "{stderr}");
    let saved = std::fs::read_to_string(output).unwrap();
    assert!(saved.contains("7g7f 3c3d 25 10 0"), "{saved}");
}

#[test]
fn omitting_input_rejects_a_missing_database_without_creating_it() {
    let directory = tempfile::tempdir().unwrap();
    let config = write_config(directory.path());
    let database = directory.path().join("missing.sqlite");
    let output = directory.path().join("output.db");

    let result = run_without_input(&config, Some(&database), &output);

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("does not exist"), "{stderr}");
    assert!(!database.exists());
    assert!(!output.exists());
}

#[test]
fn omitting_input_rejects_an_uninitialized_database_without_modifying_it() {
    let directory = tempfile::tempdir().unwrap();
    let config = write_config(directory.path());
    let database = directory.path().join("uninitialized.sqlite");
    let output = directory.path().join("output.db");
    std::fs::File::create(&database).unwrap();

    let result = run_without_input(&config, Some(&database), &output);

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("is not initialized"), "{stderr}");
    assert_eq!(std::fs::metadata(&database).unwrap().len(), 0);
    assert!(!output.exists());
}

#[test]
fn omitting_input_rejects_an_initialized_database_without_positions() {
    let directory = tempfile::tempdir().unwrap();
    let config = write_config(directory.path());
    let database = directory.path().join("empty.sqlite");
    let output = directory.path().join("output.db");
    drop(SqliteOpeningBook::open(&database, false).unwrap());

    let result = run_without_input(&config, Some(&database), &output);

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("has no positions"), "{stderr}");
    assert!(!output.exists());
}

#[test]
fn omitting_input_requires_an_explicit_database_path() {
    let directory = tempfile::tempdir().unwrap();
    let config = write_config(directory.path());
    let output = directory.path().join("output.db");

    let result = run_without_input(&config, None, &output);

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("--database is required when --input is omitted"),
        "{stderr}"
    );
    assert!(!output.exists());
}
