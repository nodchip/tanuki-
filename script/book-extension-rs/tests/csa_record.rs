use std::panic::{AssertUnwindSafe, catch_unwind};

use book_extension_runtime::{
    STARTPOS_SFEN,
    csa::{csa_source_sha256, parse_csa},
    record::RecordErrorKind,
};

#[test]
fn parses_startpos_game_and_records_every_pre_move_position() {
    let text = b"V2.2\nN+Black\nN-White\nPI\n+\n+7776FU\nT1\n-3334FU\n%TORYO\n";
    let game = parse_csa(text, "game.csa").expect("valid CSA");

    assert_eq!(game.players, ["Black", "White"]);
    assert_eq!(game.moves, ["7g7f", "3c3d"]);
    assert_eq!(game.positions.len(), 2);
    assert_eq!(game.positions[0].sfen, STARTPOS_SFEN);
    assert_eq!(game.positions[0].move_usi, "7g7f");
    assert_eq!(
        game.positions[1].sfen,
        "lnsgkgsnl/1r5b1/ppppppppp/9/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL w - 2"
    );
    assert_eq!(game.positions[1].move_usi, "3c3d");
    assert_eq!(game.endgame, "%TORYO");
}

#[test]
fn rejects_time_before_the_first_move_without_panicking() {
    let text = b"V2.2\nPI\n+\nT0\n+7776FU\n%TORYO\n";
    let result = catch_unwind(AssertUnwindSafe(|| parse_csa(text, "time.csa")));
    let error = result.expect("parser must not panic").unwrap_err();

    assert_eq!(error.kind, RecordErrorKind::TimeBeforeFirstMove);
    assert_eq!(error.line, Some(4));
}

#[test]
fn rejects_score_before_the_first_move_without_panicking() {
    let text = b"V2.2\nPI\n+\n** 0\n+7776FU\n%TORYO\n";
    let result = catch_unwind(AssertUnwindSafe(|| parse_csa(text, "score.csa")));
    let error = result.expect("parser must not panic").unwrap_err();

    assert_eq!(error.kind, RecordErrorKind::ScoreBeforeFirstMove);
    assert_eq!(error.line, Some(4));
}

#[test]
fn rejects_turn_before_initial_position_without_panicking() {
    let text = b"V2.2\n+\nPI\n+7776FU\n%TORYO\n";
    let result = catch_unwind(AssertUnwindSafe(|| parse_csa(text, "turn.csa")));
    let error = result.expect("parser must not panic").unwrap_err();

    assert_eq!(error.kind, RecordErrorKind::TurnBeforePosition);
    assert_eq!(error.line, Some(2));
}

#[test]
fn parses_wcsc_move_lines_with_inline_time_suffix() {
    let text = b"V3.0\nPI\n+\n+7776FU,T0\n-3334FU,T12\n%TORYO,T0\n";
    let game = parse_csa(text, "wcsc.csa").expect("valid WCSC CSA");

    assert_eq!(game.moves, ["7g7f", "3c3d"]);
    assert_eq!(game.endgame, "%TORYO");
}

#[test]
fn source_fingerprint_normalizes_decodable_line_endings_like_python() {
    let lf = b"V2.2\nPI\n+\nT0\n+7776FU\n%TORYO\n";
    let crlf = b"V2.2\r\nPI\r\n+\r\nT0\r\n+7776FU\r\n%TORYO\r\n";
    assert_eq!(csa_source_sha256(lf), csa_source_sha256(crlf));
}

#[test]
fn normalizes_nonstandard_terminal_suffix_without_storing_comment_text() {
    let text = b"V2.2\nPI\n+\n+7776FU\n%KACHI,'* 0\n";
    let game = parse_csa(text, "terminal-suffix.csa").expect("replayable CSA");
    assert_eq!(game.endgame, "%KACHI");
}
