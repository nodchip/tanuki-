use book_extension_runtime::book::{BookEntry, OpeningBook};
use book_extension_runtime::storage::{SaveError, save_validated_atomic};
use book_extension_runtime::validation::{IssueKind, parse_position, validate_book};
use shogi_legality_lite::all_legal_moves_partial;

const STARTPOS: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";

#[test]
fn parses_omitted_depth_and_visits_and_writes_stable_eval_order() {
    let text = format!("#YANEURAOU-DB2016 1.00\nsfen {STARTPOS}\n7g7f 3c3d 10\n2g2f 8c8d 30 4 5\n");

    let book = OpeningBook::from_text(&text, false).expect("book parses");
    let position = book.position(STARTPOS).expect("position exists");
    assert_eq!(position.entries[0].depth, 0);
    assert_eq!(position.entries[0].visits, 0);
    assert_eq!(
        book.to_text(),
        format!("#YANEURAOU-DB2016 1.00\nsfen {STARTPOS}\n2g2f 8c8d 30 4 5\n7g7f 3c3d 10 0 0\n")
    );
}

#[test]
fn ignore_ply_merges_positions_and_outputs_zero_ply() {
    let second = STARTPOS.strip_suffix(" 1").unwrap().to_owned() + " 42";
    let text = format!(
        "#YANEURAOU-DB2016 1.00\nsfen {STARTPOS}\n7g7f 3c3d 10\nsfen {second}\n2g2f 8c8d 20\n"
    );

    let book = OpeningBook::from_text(&text, true).expect("book parses");
    assert_eq!(book.positions_len(), 1);
    assert!(book.to_text().contains(" b - 0\n"));
}

#[test]
fn validator_reports_duplicate_illegal_move_and_illegal_response() {
    let mut book = OpeningBook::new(false);
    let position = book.ensure_position(STARTPOS).expect("valid position");
    position.entries = vec![
        BookEntry::new("7g7f", "3c3d", 10, 1, 0, 0),
        BookEntry::new("7g7f", "8c8d", 20, 2, 0, 1),
        BookEntry::new("7g7e", "none", 30, 3, 0, 2),
        BookEntry::new("2g2f", "7g7f", 40, 4, 0, 3),
    ];

    let report = validate_book(&book);
    let kinds: Vec<IssueKind> = report.issues.iter().map(|issue| issue.kind).collect();
    assert_eq!(
        kinds,
        vec![
            IssueKind::DuplicateMove,
            IssueKind::IllegalMove,
            IssueKind::IllegalResponse,
        ]
    );
    assert_eq!(report.positions, 1);
    assert_eq!(report.entries, 4);
}

#[test]
fn validator_accepts_white_drop_with_side_corrected_piece_color() {
    let sfen = "4k4/9/9/9/9/9/9/9/4K4 w p 1";
    let mut book = OpeningBook::new(false);
    book.ensure_position(sfen)
        .expect("valid position")
        .entries
        .push(BookEntry::new("P*5e", "none", 0, 0, 0, 0));

    let report = validate_book(&book);
    assert!(report.valid(), "issues: {:?}", report.issues);
}
#[test]
fn validator_covers_promotion_drop_nifu_dead_piece_and_check_evasion_boundaries() {
    let cases = [
        ("4k4/7P1/9/9/9/9/9/9/4K4 b - 1", "2b2a+", true),
        ("4k4/7P1/9/9/9/9/9/9/4K4 b - 1", "2b2a", false),
        ("4k4/9/9/9/9/9/9/9/4K4 b P 1", "P*5e", true),
        ("4k4/9/9/9/9/9/4P4/9/4K4 b P 1", "P*5e", false),
        ("4k4/9/9/9/9/9/9/9/4K4 b P 1", "P*5a", false),
        ("4r4/9/9/9/9/9/9/9/4K4 b P 1", "P*1e", false),
        // P*5b is legal when 4a remains an escape, but illegal pawn-drop mate
        // when both 4a and 6a are covered by the two rooks.
        ("4k4/9/3RG4/9/9/9/9/9/K8 b P 1", "P*5b", true),
        ("4k4/9/3RGR3/9/9/9/9/9/K8 b P 1", "P*5b", false),
    ];

    for (index, (sfen, move_usi, expected_valid)) in cases.into_iter().enumerate() {
        let mut book = OpeningBook::new(false);
        book.ensure_position(sfen)
            .unwrap_or_else(|error| panic!("case {index} SFEN: {error}"))
            .entries
            .push(BookEntry::new(move_usi, "none", 0, 0, 0, 0));
        let report = validate_book(&book);
        assert_eq!(
            report.valid(),
            expected_valid,
            "case {index}: {sfen} {move_usi}: {:?}",
            report.issues
        );
    }
}
#[test]
fn checkmated_position_has_zero_legal_moves() {
    let position = parse_position("4k4/4P4/3RGR3/9/9/9/9/9/K8 w - 2").unwrap();
    assert!(all_legal_moves_partial(&position).is_empty());
}
#[test]
fn invalid_book_does_not_replace_output_or_rotate_backup() {
    let directory = tempfile::tempdir().expect("temp directory");
    let output = directory.path().join("book.db");
    std::fs::write(&output, "existing\n").expect("seed output");
    let mut book = OpeningBook::new(false);
    book.ensure_position(STARTPOS)
        .expect("position")
        .entries
        .push(BookEntry::new("7g7e", "none", 0, 0, 0, 0));

    let error = save_validated_atomic(&book, &output, 3).expect_err("invalid book rejected");

    assert!(matches!(error, SaveError::Validation(_)));
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "existing\n");
    assert!(!directory.path().join("book.db.001.bak").exists());
    assert!(!directory.path().join("book.db.tmp").exists());
}

#[test]
fn atomic_save_retains_exactly_three_previous_generations() {
    let directory = tempfile::tempdir().expect("temp directory");
    let output = directory.path().join("book.db");
    let mut book = OpeningBook::new(false);
    book.ensure_position(STARTPOS)
        .expect("position")
        .entries
        .push(BookEntry::new("7g7f", "3c3d", 1, 1, 0, 0));

    for eval in 1..=5 {
        book.ensure_position(STARTPOS).unwrap().entries[0].eval_cp = eval;
        save_validated_atomic(&book, &output, 3).expect("save succeeds");
    }

    assert!(std::fs::read_to_string(&output).unwrap().contains(" 5 1 0"));
    assert!(
        std::fs::read_to_string(directory.path().join("book.db.001.bak"))
            .unwrap()
            .contains(" 4 1 0")
    );
    assert!(
        std::fs::read_to_string(directory.path().join("book.db.002.bak"))
            .unwrap()
            .contains(" 3 1 0")
    );
    assert!(
        std::fs::read_to_string(directory.path().join("book.db.003.bak"))
            .unwrap()
            .contains(" 2 1 0")
    );
    assert!(!directory.path().join("book.db.004.bak").exists());
}
