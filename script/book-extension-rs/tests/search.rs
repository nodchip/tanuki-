use book_extension_runtime::{
    book::{BookEntry, OpeningBook},
    python_random::PythonRandom,
    search::{
        LeafPath, PathStep, PetaFilter, SearchResult, calculate_ucb, merge_search_results,
        propagate_minimax, reserve_leaf_path, reserve_leaf_path_with_filter,
        reserve_vulnerability_leaf_path, reserve_vulnerability_leaf_path_with_filter,
        reserve_vulnerability_leaf_path_with_filter_and_random, score_to_winrate,
    },
};

const STARTPOS: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
const AFTER_7G7F: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL w - 2";

#[test]
fn ucb_zero_visits_has_no_special_infinite_priority() {
    let zero = calculate_ucb(0, 0, 0, 1.4, 600.0).unwrap();
    let positive = calculate_ucb(600, 0, 0, 1.4, 600.0).unwrap();
    assert!((zero - 0.5).abs() < 1e-12);
    assert!(positive > zero);
    assert!((score_to_winrate(0, 600.0).unwrap() - 0.5).abs() < 1e-12);
}

#[test]
fn merge_preserves_existing_eval_depth_and_updates_response_and_visits() {
    let mut book = OpeningBook::new(false);
    let position = book.ensure_position(STARTPOS).unwrap();
    position
        .entries
        .push(BookEntry::new("7g7f", "None", 12, 3, 5, 0));
    let results = vec![
        SearchResult::new("7g7f", "3c3d", 99, 10),
        SearchResult::new("2g2f", "none", -1, 8),
    ];

    merge_search_results(position, &results, Some("7g7f"));

    assert_eq!(
        position.entries[0],
        BookEntry::new("7g7f", "3c3d", 12, 3, 6, 0)
    );
    assert_eq!(
        position.entries[1],
        BookEntry::new("2g2f", "none", -1, 8, 0, 1)
    );
}

#[test]
fn minimax_negates_best_child_value_into_selected_parent_edge() {
    let mut book = OpeningBook::new(false);
    book.ensure_position(STARTPOS).unwrap().entries = vec![
        BookEntry::new("7g7f", "none", 30, 1, 1, 0),
        BookEntry::new("2g2f", "none", 0, 1, 1, 1),
    ];
    book.ensure_position(AFTER_7G7F).unwrap().entries = vec![
        BookEntry::new("3c3d", "none", 40, 1, 1, 2),
        BookEntry::new("8c8d", "none", 20, 1, 1, 3),
    ];
    let path = LeafPath {
        steps: vec![PathStep::new(STARTPOS, "7g7f")],
        leaf_sfen: AFTER_7G7F.to_owned(),
    };

    propagate_minimax(&mut book, &path).unwrap();

    assert_eq!(book.position(STARTPOS).unwrap().entries[0].eval_cp, -40);
}
#[test]
fn normal_leaf_selection_uses_ucb_and_records_full_move_path() {
    let mut book = OpeningBook::new(false);
    book.ensure_position(STARTPOS).unwrap().entries = vec![
        BookEntry::new("7g7f", "none", 100, 1, 1, 0),
        BookEntry::new("2g2f", "none", 10, 1, 1, 1),
    ];

    let path = reserve_leaf_path(
        &book,
        STARTPOS,
        2,
        1.4,
        600.0,
        &mut Default::default(),
        None,
    )
    .unwrap()
    .expect("leaf selected");

    assert_eq!(path.steps, vec![PathStep::new(STARTPOS, "7g7f")]);
    assert_eq!(path.leaf_sfen, AFTER_7G7F);
}

#[test]
fn normal_leaf_selection_backtracks_to_second_move_when_best_leaf_is_inflight() {
    let mut book = OpeningBook::new(false);
    book.ensure_position(STARTPOS).unwrap().entries = vec![
        BookEntry::new("7g7f", "none", 100, 1, 1, 0),
        BookEntry::new("2g2f", "none", 10, 1, 1, 1),
    ];
    let mut inflight = std::collections::HashSet::from([AFTER_7G7F.to_owned()]);

    let path = reserve_leaf_path(&book, STARTPOS, 2, 1.4, 600.0, &mut inflight, None)
        .unwrap()
        .expect("alternative leaf selected");

    assert_eq!(path.steps[0].move_usi, "2g2f");
    assert!(inflight.contains(&path.leaf_sfen));
}

#[test]
fn incomplete_distinct_legal_multipv_stops_at_current_position() {
    let mut book = OpeningBook::new(false);
    book.ensure_position(STARTPOS).unwrap().entries = vec![
        BookEntry::new("7g7f", "none", 100, 1, 1, 0),
        BookEntry::new("7g7f", "none", 90, 1, 1, 1),
    ];
    let mut inflight = Default::default();

    let path = reserve_leaf_path(&book, STARTPOS, 2, 1.4, 600.0, &mut inflight, None)
        .unwrap()
        .expect("root leaf selected");

    assert!(path.steps.is_empty());
    assert_eq!(path.leaf_sfen, STARTPOS);
}

#[test]
fn vulnerability_selection_forces_target_book_bestmove_on_target_side() {
    let mut book = OpeningBook::new(false);
    let mut target = OpeningBook::new(false);
    target.ensure_position(STARTPOS).unwrap().entries = vec![
        BookEntry::new("2g2f", "8c8d", 10, 1, 8, 0),
        BookEntry::new("7g7f", "3c3d", 100, 7, 9, 1),
    ];
    let mut inflight = Default::default();

    let path = reserve_vulnerability_leaf_path(
        &mut book,
        &target,
        STARTPOS,
        "black",
        4,
        1.4,
        600.0,
        &mut inflight,
        Some(10),
    )
    .unwrap()
    .expect("target leaf selected");

    assert_eq!(path.steps, vec![PathStep::new(STARTPOS, "7g7f")]);
    assert_eq!(path.leaf_sfen, AFTER_7G7F);
    assert_eq!(
        book.position(STARTPOS).unwrap().entries,
        vec![BookEntry::new("7g7f", "3c3d", 100, 7, 0, 0)]
    );
}

#[test]
fn fixed_seed_tie_break_matches_python_random_choice() {
    let mut book = OpeningBook::new(false);
    let mut target = OpeningBook::new(false);
    target.ensure_position(STARTPOS).unwrap().entries = vec![
        BookEntry::new("7g7f", "3c3d", 100, 1, 0, 0),
        BookEntry::new("2g2f", "8c8d", 100, 1, 0, 1),
    ];
    let mut random = PythonRandom::seeded(0);

    let path = reserve_vulnerability_leaf_path_with_filter_and_random(
        &mut book,
        &target,
        STARTPOS,
        "black",
        2,
        1.4,
        600.0,
        &mut Default::default(),
        Some(10),
        None,
        Some(&mut random),
    )
    .unwrap()
    .unwrap();

    // CPython random.Random(0).choice([0, 1]) == 1.
    assert_eq!(path.steps[0].move_usi, "2g2f");
}
#[test]
fn peta_filter_forces_best_eval_on_book_side_even_when_other_move_has_higher_ucb() {
    let mut book = OpeningBook::new(false);
    book.ensure_position(STARTPOS).unwrap().entries = vec![
        BookEntry::new("7g7f", "none", 10, 1, 0, 0),
        BookEntry::new("2g2f", "none", 100, 1, 100, 1),
    ];
    let mut inflight = Default::default();
    let filter = PetaFilter {
        book_side: "black",
        root_best_eval: 100,
        eval_diff: 50,
    };

    let path = reserve_leaf_path_with_filter(
        &book,
        STARTPOS,
        2,
        10.0,
        600.0,
        &mut inflight,
        None,
        Some(&filter),
    )
    .unwrap()
    .unwrap();

    assert_eq!(path.steps[0].move_usi, "2g2f");
}

#[test]
fn peta_filter_removes_attack_side_move_below_root_threshold() {
    let mut book = OpeningBook::new(false);
    book.ensure_position(AFTER_7G7F).unwrap().entries = vec![
        BookEntry::new("3c3d", "none", 40, 1, 0, 0),
        BookEntry::new("8c8d", "none", 70, 1, 100, 1),
    ];
    let mut inflight = Default::default();
    let filter = PetaFilter {
        book_side: "black",
        root_best_eval: 100,
        eval_diff: 50,
    };

    let path = reserve_leaf_path_with_filter(
        &book,
        AFTER_7G7F,
        2,
        10.0,
        600.0,
        &mut inflight,
        None,
        Some(&filter),
    )
    .unwrap()
    .unwrap();

    assert_eq!(path.steps[0].move_usi, "8c8d");
}

#[test]
fn vulnerability_attack_side_filters_moves_below_root_threshold() {
    let mut book = OpeningBook::new(false);
    book.ensure_position(AFTER_7G7F).unwrap().entries = vec![
        BookEntry::new("3c3d", "none", 40, 1, 0, 0),
        BookEntry::new("8c8d", "none", 70, 1, 100, 1),
    ];
    let target = OpeningBook::new(false);
    let filter = PetaFilter {
        book_side: "black",
        root_best_eval: 100,
        eval_diff: 50,
    };
    let mut inflight = Default::default();

    let path = reserve_vulnerability_leaf_path_with_filter(
        &mut book,
        &target,
        AFTER_7G7F,
        "black",
        2,
        10.0,
        600.0,
        &mut inflight,
        Some(10),
        Some(&filter),
    )
    .unwrap()
    .unwrap();

    assert_eq!(path.steps[0].move_usi, "8c8d");
}
