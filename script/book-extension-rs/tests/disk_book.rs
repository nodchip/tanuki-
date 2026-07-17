use std::fs;

use book_extension_runtime::disk_book::{DiskOpeningBook, TargetBook, TargetBookError};

#[test]
fn finds_only_the_requested_position_by_binary_search() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target.db");
    fs::write(
        &path,
        "#YANEURAOU-DB2016 1.00\n\
sfen aaa b - 0\nfirst none 1 2 3\n\
sfen target w P 0\ngood reply 100 4 5\nbad none 20 6 7\n\
sfen zzz b - 0\nlast none -1 0 0\n",
    )
    .unwrap();

    let book = DiskOpeningBook::open(&path, false).unwrap();
    let entries = book.lookup("target w P 0").unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].move_usi, "good");
    assert_eq!(entries[0].response, "reply");
    assert_eq!(entries[0].eval_cp, 100);
    assert_eq!(entries[1].move_usi, "bad");
    assert_eq!(book.lookup("aaa b - 0").unwrap()[0].move_usi, "first");
    assert_eq!(book.lookup("zzz b - 0").unwrap()[0].move_usi, "last");
    assert!(book.lookup("missing b - 0").unwrap().is_empty());
}

#[test]
fn accepts_a_utf8_bom_like_the_in_memory_book_loader() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target.db");
    fs::write(
        &path,
        "\u{feff}#YANEURAOU-DB2016 1.00\nsfen target b - 1\ngood none 100 4 5\n",
    )
    .unwrap();

    let book = DiskOpeningBook::open(&path, false).unwrap();

    assert_eq!(book.lookup("target b - 1").unwrap()[0].move_usi, "good");
}

#[test]
fn ignore_ply_uses_the_normalized_position_key() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target.db");
    fs::write(
        &path,
        "#YANEURAOU-DB2016 1.00\n\
sfen aaa b - 1\nfirst none 1 2 3\n\
sfen target w P 7\ngood none 100 4 5\n",
    )
    .unwrap();

    let book = DiskOpeningBook::open(&path, true).unwrap();

    assert_eq!(book.lookup("target w P 999").unwrap()[0].move_usi, "good");
}

#[test]
fn rejects_a_target_book_that_is_not_sorted_by_lookup_key() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target.db");
    fs::write(
        &path,
        "#YANEURAOU-DB2016 1.00\n\
sfen zzz b - 0\nlast none -1 0 0\n\
sfen aaa b - 0\nfirst none 1 2 3\n",
    )
    .unwrap();

    let error = DiskOpeningBook::open(&path, false).unwrap_err();

    assert!(matches!(
        error,
        TargetBookError::Unsorted { previous, current }
            if previous == "zzz b - 0" && current == "aaa b - 0"
    ));
}

#[test]
fn rejects_malformed_entries_during_streaming_validation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("target.db");
    fs::write(
        &path,
        "#YANEURAOU-DB2016 1.00\nsfen aaa b - 0\nbroken-entry\n",
    )
    .unwrap();

    let error = DiskOpeningBook::open(&path, false).unwrap_err();

    assert!(matches!(error, TargetBookError::Parse(_)));
}
