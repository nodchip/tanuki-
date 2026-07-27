use std::process::Command;

#[test]
fn imports_and_exports_yaneuraou_book_from_command_line() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.db");
    let database = directory.path().join("book.sqlite");
    let output = directory.path().join("output.db");
    std::fs::write(
        &input,
        format!(
            "#YANEURAOU-DB2016 1.00\nsfen {}\n7g7f 3c3d 10 2 3\n",
            book_extension_runtime::STARTPOS_SFEN
        ),
    )
    .unwrap();

    let import = Command::new(env!("CARGO_BIN_EXE_book-sqlite"))
        .args([
            "import",
            "--database",
            database.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        import.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&import.stderr)
    );
    assert!(String::from_utf8_lossy(&import.stderr).contains("moves=1"));

    let export = Command::new(env!("CARGO_BIN_EXE_book-sqlite"))
        .args([
            "export",
            "--database",
            database.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert!(
        std::fs::read_to_string(output)
            .unwrap()
            .contains("7g7f 3c3d 10 2 3")
    );
}
