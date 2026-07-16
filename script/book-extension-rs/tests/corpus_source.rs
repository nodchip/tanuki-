use std::{fs::File, io::Write};

use book_extension_runtime::corpus_source::{
    ArchiveLimits, RecordFormat, SourceError, VisitRecordsError, inspect_seven_zip, read_records,
    visit_records,
};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

fn create_zip(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let file = File::create(path).unwrap();
    let mut writer = ZipWriter::new(file);
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn zip_records_are_filtered_and_returned_in_stable_path_order() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("records.zip");
    create_zip(
        &path,
        &[
            ("z/game.kif", b"kif"),
            ("ignored.txt", b"text"),
            ("./a/game.csa", b"csa"),
        ],
    );

    let records = read_records(
        &path,
        Some(r"game\.(csa|kif)$"),
        ArchiveLimits::default(),
        None,
    )
    .expect("ZIP records");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].relative_path, "records.zip!/a/game.csa");
    assert_eq!(records[0].format, RecordFormat::Csa);
    assert_eq!(records[0].bytes, b"csa");
    assert_eq!(records[1].relative_path, "records.zip!/z/game.kif");
    assert_eq!(records[1].format, RecordFormat::Kif);
}

#[test]
fn rejects_parent_traversal_before_returning_any_record() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("traversal.zip");
    create_zip(&path, &[("../escape.csa", b"bad")]);

    let error = read_records(&path, None, ArchiveLimits::default(), None).unwrap_err();
    assert!(matches!(error, SourceError::UnsafeMemberPath(_)));
}

#[test]
fn rejects_member_and_total_size_limits() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("large.zip");
    create_zip(&path, &[("a.csa", b"12345"), ("b.csa", b"67890")]);

    let member_error = read_records(
        &path,
        None,
        ArchiveLimits {
            max_members: 10,
            max_member_bytes: 4,
            max_total_bytes: 100,
        },
        None,
    )
    .unwrap_err();
    assert!(matches!(member_error, SourceError::MemberTooLarge { .. }));

    let total_error = read_records(
        &path,
        None,
        ArchiveLimits {
            max_members: 10,
            max_member_bytes: 10,
            max_total_bytes: 9,
        },
        None,
    )
    .unwrap_err();
    assert!(matches!(total_error, SourceError::TotalTooLarge { .. }));
}

#[test]
fn directory_records_are_returned_in_stable_relative_order() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("z")).unwrap();
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    std::fs::write(dir.path().join("z/game.kif"), b"kif").unwrap();
    std::fs::write(dir.path().join("a/game.csa"), b"csa").unwrap();

    let records = read_records(dir.path(), None, ArchiveLimits::default(), None).unwrap();
    assert_eq!(
        records
            .iter()
            .map(|item| item.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["a/game.csa", "z/game.kif"]
    );
}

#[test]
fn tar_xz_records_are_spooled_and_returned_in_stable_order() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("records.tar.xz");
    let file = File::create(&path).unwrap();
    let encoder = xz2::write::XzEncoder::new(file, 1);
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes) in [
        ("./z/game.csa", b"z".as_slice()),
        ("./a/game.csa", b"a".as_slice()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, bytes).unwrap();
    }
    let encoder = builder.into_inner().unwrap();
    encoder.finish().unwrap();

    let records = read_records(&path, None, ArchiveLimits::default(), None).unwrap();
    assert_eq!(
        records
            .iter()
            .map(|item| item.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["records.tar.xz!/a/game.csa", "records.tar.xz!/z/game.csa"]
    );
    assert_eq!(records[0].bytes, b"a");
    assert_eq!(records[1].bytes, b"z");
}

#[test]
fn explicit_missing_seven_zip_does_not_fall_back() {
    let dir = TempDir::new().unwrap();
    let error = inspect_seven_zip(Some(&dir.path().join("missing-7z.exe"))).unwrap_err();
    assert!(matches!(error, SourceError::SevenZipRequired));
}

#[test]
fn external_seven_zip_reader_validates_and_extracts_records() {
    let seven_zip = std::path::Path::new(r"C:\Program Files\7-Zip\7z.exe");
    if !seven_zip.is_file() {
        eprintln!("skipping: local 7z.exe is unavailable");
        return;
    }
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input");
    std::fs::create_dir_all(input.join("z")).unwrap();
    std::fs::create_dir_all(input.join("a")).unwrap();
    std::fs::write(input.join("z/game.kif"), b"kif").unwrap();
    std::fs::write(input.join("a/game.csa"), b"csa").unwrap();
    let archive = dir.path().join("records.7z");
    let status = std::process::Command::new(seven_zip)
        .current_dir(&input)
        .args(["a", "-t7z", archive.to_str().unwrap(), ".", "-y", "-bb0"])
        .status()
        .unwrap();
    assert!(status.success());

    let info = inspect_seven_zip(Some(seven_zip)).unwrap();
    assert_eq!(info.path, seven_zip);
    assert!(info.version.contains("7-Zip"));
    assert_eq!(info.sha256.len(), 64);

    let records = read_records(&archive, None, ArchiveLimits::default(), Some(seven_zip)).unwrap();
    assert_eq!(
        records
            .iter()
            .map(|item| item.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["records.7z!/a/game.csa", "records.7z!/z/game.kif"]
    );
}

#[test]
fn reads_real_lzh_when_fixture_path_is_provided() {
    let Some(path) = std::env::var_os("CORPUS_REAL_LZH") else {
        eprintln!("skipping: CORPUS_REAL_LZH is not set");
        return;
    };
    let path = std::path::PathBuf::from(path);
    let records = read_records(&path, None, ArchiveLimits::default(), None).unwrap();
    assert!(!records.is_empty());
    assert!(
        records
            .iter()
            .all(|record| record.format == RecordFormat::Kif)
    );
    assert!(
        records
            .iter()
            .all(|record| record.relative_path.contains("!/"))
    );
}

#[test]
fn visitor_receives_records_in_stable_order_without_collecting_them() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("records.zip");
    create_zip(
        &path,
        &[
            ("z/game.kif", b"kif"),
            ("a/game.csa", b"csa"),
            ("skip.csa", b"skip"),
        ],
    );
    let mut visited = Vec::new();

    let stats = visit_records(
        &path,
        Some(r"/game\."),
        ArchiveLimits::default(),
        None,
        |record| {
            visited.push((record.relative_path, record.bytes));
            Ok::<_, &'static str>(())
        },
    )
    .unwrap();

    assert_eq!(stats.record_members, 3);
    assert_eq!(stats.filtered_members, 1);
    assert_eq!(stats.visited_records, 2);
    assert_eq!(
        visited,
        [
            ("records.zip!/a/game.csa".to_owned(), b"csa".to_vec()),
            ("records.zip!/z/game.kif".to_owned(), b"kif".to_vec()),
        ]
    );
}

#[test]
fn visitor_failure_stops_iteration_and_is_returned_with_its_type() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("records.zip");
    create_zip(
        &path,
        &[("a/game.csa", b"first"), ("b/game.csa", b"second")],
    );
    let mut count = 0;

    let error = visit_records(&path, None, ArchiveLimits::default(), None, |_| {
        count += 1;
        Err("database unavailable")
    })
    .unwrap_err();

    assert_eq!(count, 1);
    assert!(matches!(
        error,
        VisitRecordsError::Visitor("database unavailable")
    ));
}
