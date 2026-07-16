use std::panic::{AssertUnwindSafe, catch_unwind};

use book_extension_runtime::{csa::parse_csa, record::RecordErrorKind};

fn dense_startpos() -> String {
    let empty = " * ".repeat(9);
    [
        "V2.2".to_owned(),
        "N+Black".to_owned(),
        "N-White".to_owned(),
        "P1-KY-KE-GI-KI-OU-KI-GI-KE-KY".to_owned(),
        "P2 * -HI *  *  *  *  * -KA * ".to_owned(),
        "P3-FU-FU-FU-FU-FU-FU-FU-FU-FU".to_owned(),
        format!("P4{empty}"),
        format!("P5{empty}"),
        format!("P6{empty}"),
        "P7+FU+FU+FU+FU+FU+FU+FU+FU+FU".to_owned(),
        "P8 * +KA *  *  *  *  * +HI * ".to_owned(),
        "P9+KY+KE+GI+KI+OU+KI+GI+KE+KY".to_owned(),
        "+".to_owned(),
        "+7776FU".to_owned(),
        "%TORYO".to_owned(),
    ]
    .join("\n")
        + "\n"
}

#[test]
fn accepts_fully_initialized_dense_startpos() {
    let game = parse_csa(dense_startpos().as_bytes(), "dense.csa").expect("dense startpos");
    assert_eq!(game.moves, ["7g7f"]);
}

#[test]
fn rejects_unknown_dense_piece_token_without_panicking() {
    let text = dense_startpos().replacen("-KY", "-XX", 1);
    let result = catch_unwind(AssertUnwindSafe(|| parse_csa(text.as_bytes(), "token.csa")));
    let error = result.expect("parser must not panic").unwrap_err();
    assert_eq!(error.kind, RecordErrorKind::InvalidPositionToken);
    assert_eq!(error.line, Some(4));
}

#[test]
fn parses_00al_without_panicking_and_excludes_non_startpos() {
    let text = b"V2.2\nP+59OU00AL\nP-51OU\n+\n+0058HI\n%TORYO\n";
    let result = catch_unwind(AssertUnwindSafe(|| parse_csa(text, "all.csa")));
    let error = result.expect("parser must not panic").unwrap_err();
    assert_eq!(error.kind, RecordErrorKind::NonStartpos);
}

#[test]
fn sparse_unspecified_squares_are_initialized_as_empty() {
    let text = b"V2.2\nP+59OU\nP-51OU\n+\n+5958OU\n%TORYO\n";
    let result = catch_unwind(AssertUnwindSafe(|| parse_csa(text, "sparse.csa")));
    let error = result.expect("parser must not panic").unwrap_err();
    assert_eq!(error.kind, RecordErrorKind::NonStartpos);
}
