use std::collections::HashSet;

use book_extension_runtime::{
    book::{BookEntry, OpeningBook},
    corpus::CorpusStore,
    python_random::PythonRandom,
    search::{LeafPath, PathStep, calculate_ucb, propagate_minimax, reserve_leaf_path},
    usi::{PositionRoot, build_position_command},
    validation::legal_distinct_entry_count,
};
use serde_json::json;

const STARTPOS: &str = book_extension_runtime::STARTPOS_SFEN;
const AFTER_7G7F: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL w - 2";

fn main() {
    let fixture =
        format!("#YANEURAOU-DB2016 1.00\nsfen {STARTPOS}\n7g7f 3c3d 100 1 1\n2g2f 8c8d 10 1 1\n");
    let book = OpeningBook::from_text(&fixture, false).unwrap();
    let ignore = OpeningBook::from_text(&fixture, true).unwrap();
    let mut inflight = HashSet::new();
    let path = reserve_leaf_path(&book, STARTPOS, 2, 1.4, 600.0, &mut inflight, None)
        .unwrap()
        .unwrap();

    let mut minimax = book.clone();
    minimax.ensure_position(AFTER_7G7F).unwrap().entries = vec![
        BookEntry::new("3c3d", "none", 40, 1, 1, 2),
        BookEntry::new("8c8d", "none", 20, 1, 1, 3),
    ];
    propagate_minimax(
        &mut minimax,
        &LeafPath {
            steps: vec![PathStep::new(STARTPOS, "7g7f")],
            leaf_sfen: AFTER_7G7F.to_owned(),
        },
    )
    .unwrap();
    let corpus_directory = tempfile::tempdir().unwrap();
    let mut corpus = CorpusStore::open(&corpus_directory.path().join("corpus.sqlite")).unwrap();
    corpus
        .upsert_candidate("p", "7g7f", &["7g7f"], "wcsc:event", "100")
        .unwrap();
    corpus
        .upsert_candidate("p", "2g2f", &["2g2f"], "denryu:event", "200")
        .unwrap();
    let corpus_candidate = corpus
        .reserve_for_position("snapshot", "p", &HashSet::new(), 1, 60.0, None, 1000.0)
        .unwrap()
        .unwrap();
    let mut random = PythonRandom::seeded(42);
    let choices: Vec<_> = (0..10).map(|_| random.choice_index(3).unwrap()).collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "roundtrip": book.to_text(),
            "ignore_key": ignore.position_key(STARTPOS),
            "distinct_legal": legal_distinct_entry_count(
                STARTPOS,
                &[
                    BookEntry::new("7g7f", "3c3d", 0, 0, 0, 0),
                    BookEntry::new("7g7f", "8c8d", 0, 0, 0, 1),
                    BookEntry::new("7g7e", "none", 0, 0, 0, 2),
                    BookEntry::new("2g2f", "8c8d", 0, 0, 0, 3),
                ],
            ),
            "ucb": format!("{:.12}", calculate_ucb(100, 1, 2, 1.4, 600.0).unwrap()),
            "leaf_moves": path.steps.iter().map(|step| &step.move_usi).collect::<Vec<_>>(),
            "leaf_sfen": path.leaf_sfen,
            "minimax_root_eval": minimax.position(STARTPOS).unwrap().entries[0].eval_cp,
            "position": build_position_command(&PositionRoot::Startpos, &["7g7f", "3c3d"]),
            "random_choices": choices,
            "corpus_move": corpus_candidate.move_usi,
            "corpus_status": corpus_candidate.status.as_str(),
            "corpus_attempts": corpus_candidate.attempts,
            "corpus_width": corpus.progressive_width().unwrap(),
        }))
        .unwrap()
    );
}
