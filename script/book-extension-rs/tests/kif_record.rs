use std::panic::{AssertUnwindSafe, catch_unwind};

use book_extension_runtime::{kif::parse_kif, record::RecordErrorKind};

#[test]
fn parses_origin_same_promotion_drop_and_resignation() {
    let text = "棋戦：fixture\n手合割：平手\n先手：Black\n後手：White\n手数----指手---------消費時間--\n 1 ７六歩(77) ( 0:01/00:00:01)\n 2 ３四歩(33) ( 0:01/00:00:01)\n 3 ２二角成(88) ( 0:01/00:00:02)\n 4 同　銀(31) ( 0:01/00:00:02)\n 5 ４五角打 ( 0:01/00:00:03)\n 6 投了 ( 0:00/00:00:02)\nまで5手で先手の勝ち\n";
    let games = parse_kif(text.as_bytes(), "game.kif").expect("valid KIF");

    assert_eq!(games.len(), 1);
    let game = &games[0];
    assert_eq!(game.players, ["Black", "White"]);
    assert_eq!(game.moves, ["7g7f", "3c3d", "8h2b+", "3a2b", "B*4e"]);
    assert_eq!(game.positions.len(), 5);
    assert_eq!(game.endgame, "%TORYO");
    assert_eq!(
        game.sha256,
        "b38993985ae50311e4043dcfcbf3136a1f46c9c297667b8b3846458fa2c601ea"
    );
}

#[test]
fn rejects_handicap_before_replaying_moves() {
    let text = "手合割：香落ち\n先手：Black\n後手：White\n手数----指手---------消費時間--\n 1 ７六歩(77)\n 2 投了\n";
    let error = parse_kif(text.as_bytes(), "handicap.kif").unwrap_err();
    assert_eq!(error.kind, RecordErrorKind::NonStartpos);
}

#[test]
fn malformed_kif_never_panics() {
    let text = b"\xff\x00\x81\n1 ???\n";
    let result = catch_unwind(AssertUnwindSafe(|| parse_kif(text, "broken.kif")));
    assert!(result.expect("parser must not panic").is_err());
}

#[test]
fn maps_denryu_terminal_notation_to_csa_result_codes() {
    for (notation, expected) in [
        ("切れ負け", "%TIME_UP"),
        ("入玉勝ち", "%KACHI"),
        ("反則負け", "%ILLEGAL_MOVE"),
        ("反則勝ち", "%ILLEGAL_MOVE"),
    ] {
        let text = format!(
            "手合割：平手\n先手：Black\n後手：White\n手数----指手---------消費時間--\n 1 ７六歩(77)\n 2 {notation}\n"
        );
        let games = parse_kif(text.as_bytes(), "terminal.kif").expect(notation);
        assert_eq!(games[0].endgame, expected, "{notation}");
    }
}

#[test]
fn parses_multiple_games_from_one_kif_file() {
    let text = "#KIF version=2.0 encoding=UTF-8\n手合割：平手\n先手：Black1\n後手：White1\n手数----指手---------消費時間--\n 1 ７六歩(77)\n 2 投了\nまで1手で先手の勝ち\n\n#KIF version=2.0 encoding=UTF-8\n手合割：平手\n先手：Black2\n後手：White2\n手数----指手---------消費時間--\n 1 ２六歩(27)\n 2 投了\nまで1手で先手の勝ち\n";

    let games = parse_kif(text.as_bytes(), "multi.kif").expect("two valid games");

    assert_eq!(games.len(), 2);
    assert_eq!(games[0].players, ["Black1", "White1"]);
    assert_eq!(games[0].moves, ["7g7f"]);
    assert_eq!(games[0].source_path, "multi.kif#game=1");
    assert_eq!(games[1].players, ["Black2", "White2"]);
    assert_eq!(games[1].moves, ["2g2f"]);
    assert_eq!(games[1].source_path, "multi.kif#game=2");
}

#[test]
fn hashes_the_same_canonical_csa_as_the_python_reference() {
    let text = "手合割：平手\n先手：Alpha\n後手：Beta\n手数----指手---------消費時間--\n1 ７六歩(77)\n2 ３四歩(33)\n3 投了\n";
    let games = parse_kif(text.as_bytes(), "game.kif").expect("valid KIF");

    assert_eq!(
        games[0].sha256,
        "2401978a0270a2053697ccb3e0beb5d0be0196b303ee0126ac5ff3c0656aab14"
    );
}
