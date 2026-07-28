use book_extension_runtime::{
    search::{LeafPath, PathStep, SearchResult},
    sqlite_book::{SqliteOpeningBook, export_yaneuraou_atomic},
};

const STARTPOS: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
const AFTER_7G7F: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL w - 2";

#[test]
fn later_release_adds_only_missing_positions_and_moves() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("book.sqlite");
    let base = directory.path().join("base.db");
    let update = directory.path().join("update.db");
    let output = directory.path().join("output.db");
    std::fs::write(
        &base,
        format!("#YANEURAOU-DB2016 1.00\nsfen {STARTPOS}\n7g7f 3c3d 100 4 11\n"),
    )
    .unwrap();
    std::fs::write(
        &update,
        format!(
            "#YANEURAOU-DB2016 1.00\nsfen {AFTER_7G7F}\n3c3d 2g2f 20 2 3\nsfen {STARTPOS}\n7g7f 3c3d 999 99 999\n2g2f 8c8d 50 5 7\n"
        ),
    )
    .unwrap();

    let mut book = SqliteOpeningBook::open(&database, false).unwrap();
    let first = book.import_yaneuraou(&base).unwrap();
    assert_eq!((first.inserted_positions, first.inserted_moves), (1, 1));
    let second = book.import_yaneuraou(&update).unwrap();
    assert_eq!((second.inserted_positions, second.inserted_moves), (1, 2));
    let duplicate = book.import_yaneuraou(&update).unwrap();
    assert!(duplicate.already_imported);

    let root = book.position(STARTPOS).unwrap().unwrap();
    let existing = root.find_entry("7g7f").unwrap();
    assert_eq!(
        (existing.eval_cp, existing.depth, existing.visits),
        (100, 4, 11)
    );
    assert_eq!(root.find_entry("2g2f").unwrap().eval_cp, 50);
    export_yaneuraou_atomic(&database, &output, 0).unwrap();
    let text = std::fs::read_to_string(output).unwrap();
    assert!(text.find(AFTER_7G7F).unwrap() < text.find(STARTPOS).unwrap());
}

#[test]
fn search_updates_eval_depth_response_visits_and_minimax_in_one_transaction() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("book.sqlite");
    let input = directory.path().join("input.db");
    std::fs::write(
        &input,
        format!(
            "#YANEURAOU-DB2016 1.00\nsfen {AFTER_7G7F}\n3c3d none 20 2 3\nsfen {STARTPOS}\n7g7f 3c3d 100 4 11\n"
        ),
    )
    .unwrap();
    let mut book = SqliteOpeningBook::open(&database, false).unwrap();
    book.import_yaneuraou(&input).unwrap();
    let path = LeafPath {
        steps: vec![PathStep::new(STARTPOS, "7g7f")],
        leaf_sfen: AFTER_7G7F.to_owned(),
    };

    assert!(
        !book
            .apply_search_results(&path, &[SearchResult::new("3c3d", "7g7f", 80, 9)],)
            .unwrap()
    );

    let child = book.position(AFTER_7G7F).unwrap().unwrap();
    let child_move = child.find_entry("3c3d").unwrap();
    assert_eq!(
        (
            child_move.response.as_str(),
            child_move.eval_cp,
            child_move.depth,
            child_move.visits,
        ),
        ("7g7f", 80, 9, 3)
    );
    let root = book.position(STARTPOS).unwrap().unwrap();
    let root_move = root.find_entry("7g7f").unwrap();
    assert_eq!((root_move.eval_cp, root_move.visits), (-80, 12));
}

#[test]
fn opening_existing_database_rejects_ignore_ply_mismatch() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("book.sqlite");
    drop(SqliteOpeningBook::open(&database, true).unwrap());

    let error = SqliteOpeningBook::open_existing(&database, false)
        .err()
        .expect("mismatched mode rejected");
    assert!(error.to_string().contains("ignore_ply=true"));
}

#[test]
fn ignore_ply_merges_keys_and_exports_a_zero_ply_sfen() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("book.sqlite");
    let input = directory.path().join("input.db");
    let output = directory.path().join("output.db");
    let ply_two = STARTPOS.rsplit_once(' ').unwrap().0.to_owned() + " 2";
    std::fs::write(
        &input,
        format!(
            "#YANEURAOU-DB2016 1.00\nsfen {STARTPOS}\n7g7f 3c3d 10 1 2\nsfen {ply_two}\n2g2f 8c8d 20 2 3\n"
        ),
    )
    .unwrap();
    let mut book = SqliteOpeningBook::open(&database, true).unwrap();

    book.import_yaneuraou(&input).unwrap();
    assert_eq!(book.positions_len().unwrap(), 1);
    assert_eq!(book.position(STARTPOS).unwrap().unwrap().entries.len(), 2);
    export_yaneuraou_atomic(&database, &output, 0).unwrap();
    let text = std::fs::read_to_string(output).unwrap();
    assert!(text.contains(&format!(
        "sfen {} 0\n",
        STARTPOS.rsplit_once(' ').unwrap().0
    )));
    assert_eq!(text.matches("sfen ").count(), 1);
}
