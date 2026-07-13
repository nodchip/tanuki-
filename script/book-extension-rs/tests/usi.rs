use book_extension_runtime::search::SearchResult;
use book_extension_runtime::usi::{
    PositionRoot, build_position_command, parse_info_line, parse_score,
};

#[test]
fn parses_multipv_and_ignores_bound_unless_allowed() {
    assert_eq!(
        parse_info_line(
            "info depth 12 score cp -99 multipv 2 pv 2g2f 8c8d 2f2e",
            false
        )
        .unwrap(),
        Some((2, SearchResult::new("2g2f", "8c8d", -99, 12)))
    );
    assert_eq!(
        parse_info_line(
            "info depth 9 score cp 30 lowerbound multipv 1 pv 7g7f",
            false
        )
        .unwrap(),
        None
    );
    assert_eq!(
        parse_info_line(
            "info depth 9 score cp 30 lowerbound multipv 1 pv 7g7f",
            true
        )
        .unwrap(),
        Some((1, SearchResult::new("7g7f", "none", 30, 9)))
    );
}

#[test]
fn repetition_candidate_keeps_complete_history_in_position_command() {
    let moves = [
        "5i5h", "5a5b", "5h5i", "5b5a", "5i5h", "5a5b", "5h5i", "5b5a",
    ];
    assert_eq!(
        build_position_command(
            &PositionRoot::Sfen("4k4/9/9/9/9/9/9/9/4K4 b - 1".to_owned()),
            &moves,
        ),
        "position sfen 4k4/9/9/9/9/9/9/9/4K4 b - 1 moves 5i5h 5a5b 5h5i 5b5a 5i5h 5a5b 5h5i 5b5a"
    );
}
#[test]
fn converts_cp_and_mate_scores_like_python_runtime() {
    assert_eq!(parse_score("cp", "-15").unwrap(), -15);
    assert_eq!(parse_score("mate", "+").unwrap(), 100_000);
    assert_eq!(parse_score("mate", "-").unwrap(), -100_000);
    assert_eq!(parse_score("mate", "3").unwrap(), 99_997);
    assert_eq!(parse_score("mate", "-3").unwrap(), -99_997);
}

#[test]
fn builds_startpos_and_arbitrary_root_commands_with_full_history() {
    assert_eq!(
        build_position_command(&PositionRoot::Startpos, &["7g7f", "3c3d"]),
        "position startpos moves 7g7f 3c3d"
    );
    assert_eq!(
        build_position_command(
            &PositionRoot::Sfen("4k4/9/9/9/9/9/9/9/4K4 b - 1".to_owned()),
            &["5i5h"]
        ),
        "position sfen 4k4/9/9/9/9/9/9/9/4K4 b - 1 moves 5i5h"
    );
    assert_eq!(
        build_position_command(&PositionRoot::Startpos, &[]),
        "position startpos"
    );
}
