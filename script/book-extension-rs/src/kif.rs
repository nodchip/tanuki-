use encoding_rs::SHIFT_JIS;
use regex::Regex;
use sha2::{Digest, Sha256};
use shogi_core::{Color, Move, PartialPosition, Piece, PieceKind, Square, ToUsi};
use shogi_legality_lite::{all_legal_moves_partial, is_legal_partial};

use crate::record::{CorpusPosition, NormalizedGame, RecordError, RecordErrorKind};

pub fn parse_kif(bytes: &[u8], source_path: &str) -> Result<Vec<NormalizedGame>, RecordError> {
    let text = decode_record(bytes, source_path)?;
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let sections = split_kif_sections(&normalized);
    let multiple = sections.len() > 1;
    sections
        .into_iter()
        .enumerate()
        .map(|(index, (section, line_offset))| {
            let game_source = if multiple {
                format!("{source_path}#game={}", index + 1)
            } else {
                source_path.to_owned()
            };
            parse_kif_game(&section, &game_source, line_offset)
        })
        .collect()
}

fn parse_kif_game(
    normalized: &str,
    source_path: &str,
    line_offset: usize,
) -> Result<NormalizedGame, RecordError> {
    let move_line =
        Regex::new(r"^\s*(\d+)\s+(.+?)(?:\s+\(\s*\d+:\d{2}/.*)?$").map_err(|error| {
            RecordError::new(
                source_path,
                RecordErrorKind::InvalidMove,
                None,
                error.to_string(),
            )
        })?;
    let mut players = [String::new(), String::new()];
    let mut even_game = false;
    let mut in_moves = false;
    let mut position = PartialPosition::startpos();
    let mut moves = Vec::new();
    let mut positions = Vec::new();
    let mut last_to = None;
    let mut endgame = None;
    let mut canonical_moves = Vec::new();

    for (index, raw_line) in normalized.lines().enumerate() {
        let line_number = line_offset + index + 1;
        let line = raw_line.trim();
        if let Some(value) = header_value(line, "手合割") {
            if value != "平手" {
                return Err(RecordError::new(
                    source_path,
                    RecordErrorKind::NonStartpos,
                    Some(line_number),
                    format!("unsupported KIF handicap or initial position: {value}"),
                ));
            }
            even_game = true;
            continue;
        }
        if let Some(value) = header_value(line, "先手") {
            players[0] = value.to_owned();
            continue;
        }
        if let Some(value) = header_value(line, "後手") {
            players[1] = value.to_owned();
            continue;
        }
        if line.starts_with("手数----") {
            in_moves = true;
            continue;
        }
        if !in_moves || line.is_empty() || line.starts_with('*') || line.starts_with("まで") {
            continue;
        }
        let Some(captures) = move_line.captures(raw_line) else {
            continue;
        };
        let notation = captures.get(2).expect("capture exists").as_str().trim();
        if let Some(marker) = terminal_marker(notation) {
            endgame = Some(marker.to_owned());
            break;
        }
        let previous_sfen = position.to_sfen_owned();
        let move_value = resolve_move(notation, &position, last_to).map_err(|detail| {
            RecordError::new(
                source_path,
                RecordErrorKind::InvalidMove,
                Some(line_number),
                detail,
            )
            .with_position(previous_sfen.clone(), notation)
        })?;
        let move_usi = move_value.to_usi_owned();
        canonical_moves.push(move_to_csa(move_value, &position).map_err(|detail| {
            RecordError::new(
                source_path,
                RecordErrorKind::InvalidMove,
                Some(line_number),
                detail,
            )
            .with_position(previous_sfen.clone(), notation)
        })?);
        positions.push(CorpusPosition {
            sfen: previous_sfen.clone(),
            move_usi: move_usi.clone(),
            ply: moves.len(),
        });
        position.make_move(move_value).ok_or_else(|| {
            RecordError::new(
                source_path,
                RecordErrorKind::IllegalMove,
                Some(line_number),
                "failed to apply resolved KIF move",
            )
            .with_position(previous_sfen, notation)
        })?;
        last_to = Some(move_value.to());
        moves.push(move_usi);
    }

    if !even_game {
        return Err(RecordError::new(
            source_path,
            RecordErrorKind::NonStartpos,
            None,
            "KIF does not declare an even game",
        ));
    }
    if moves.is_empty() {
        return Err(RecordError::new(
            source_path,
            RecordErrorKind::NoMoves,
            None,
            "game has no recorded moves",
        ));
    }
    let endgame = endgame.ok_or_else(|| {
        RecordError::new(
            source_path,
            RecordErrorKind::MissingEndgame,
            None,
            "KIF has no recognized terminal move",
        )
    })?;
    // The schema-v5 Python pipeline hashes the canonical CSA generated by cshogi,
    // not the original KIF bytes. cshogi 0.9.7 emits CHUDAN for KIF results.
    let mut canonical = format!("V2.2\nN+{}\nN-{}\nPI\n+\n", players[0], players[1]);
    for move_line in canonical_moves {
        canonical.push_str(&move_line);
        canonical.push('\n');
    }
    canonical.push_str(python_reference_endgame(normalized));
    canonical.push('\n');
    if std::env::var_os("CORPUS_DEBUG_CANONICAL").is_some() {
        eprintln!("{canonical}");
    }
    let sha256 = format!("{:x}", Sha256::digest(canonical.as_bytes()));
    Ok(NormalizedGame {
        source_path: source_path.to_owned(),
        sha256,
        players,
        moves,
        positions,
        endgame,
    })
}

fn python_reference_endgame(normalized: &str) -> &'static str {
    for line in normalized.lines() {
        let Some(mut result) = line.trim_start_matches([' ', '　']).strip_prefix("まで") else {
            continue;
        };
        result = result.strip_prefix('、').unwrap_or(result);
        let digit_bytes = result
            .char_indices()
            .take_while(|(_, character)| character.is_ascii_digit())
            .map(|(index, character)| index + character.len_utf8())
            .last()
            .unwrap_or(0);
        if digit_bytes == 0 {
            continue;
        }
        let Some(result) = result[digit_bytes..].strip_prefix("手で") else {
            continue;
        };
        if result.starts_with("先手の反則負け")
            || result.starts_with("後手の反則負け")
            || result.starts_with("下手の反則負け")
            || result.starts_with("上手の反則負け")
        {
            return "%ILLEGAL_MOVE";
        }
        if result.starts_with("先手の反則勝ち") || result.starts_with("下手の反則勝ち")
        {
            return "%+ILLEGAL_ACTION";
        }
        if result.starts_with("後手の反則勝ち") || result.starts_with("上手の反則勝ち")
        {
            return "%-ILLEGAL_ACTION";
        }
        if result.starts_with("先手の入玉勝ち")
            || result.starts_with("後手の入玉勝ち")
            || result.starts_with("下手の入玉勝ち")
            || result.starts_with("上手の入玉勝ち")
        {
            return "%KACHI";
        }
        if result.starts_with("先手の勝ち")
            || result.starts_with("後手の勝ち")
            || result.starts_with("下手の勝ち")
            || result.starts_with("上手の勝ち")
        {
            return "%TORYO";
        }
        if result.starts_with("中断") {
            return "%CHUDAN";
        }
        if result.starts_with("千日手") || result.starts_with("持将棋") {
            // cshogi 0.9.7 maps both draw summaries to SENNICHITE.
            return "%SENNICHITE";
        }
    }
    "%CHUDAN"
}
fn move_to_csa(move_value: Move, position: &PartialPosition) -> Result<String, &'static str> {
    let sign = match position.side_to_move() {
        Color::Black => '+',
        Color::White => '-',
    };
    let (from_file, from_rank, to, kind) = match move_value {
        Move::Normal { from, to, .. } => (
            from.file(),
            from.rank(),
            to,
            move_result_kind(position, move_value).ok_or("missing moving piece")?,
        ),
        Move::Drop { piece, to } => (0, 0, to, piece.piece_kind()),
    };
    Ok(format!(
        "{sign}{from_file}{from_rank}{}{}{}",
        to.file(),
        to.rank(),
        csa_piece_code(kind)
    ))
}

fn csa_piece_code(kind: PieceKind) -> &'static str {
    match kind {
        PieceKind::Pawn => "FU",
        PieceKind::Lance => "KY",
        PieceKind::Knight => "KE",
        PieceKind::Silver => "GI",
        PieceKind::Gold => "KI",
        PieceKind::Bishop => "KA",
        PieceKind::Rook => "HI",
        PieceKind::King => "OU",
        PieceKind::ProPawn => "TO",
        PieceKind::ProLance => "NY",
        PieceKind::ProKnight => "NK",
        PieceKind::ProSilver => "NG",
        PieceKind::ProBishop => "UM",
        PieceKind::ProRook => "RY",
    }
}
fn split_kif_sections(normalized: &str) -> Vec<(String, usize)> {
    let mut sections = Vec::new();
    let mut current = String::new();
    let mut current_start = 0_usize;
    let mut saw_game_header = false;
    for (index, line) in normalized.lines().enumerate() {
        let is_game_header = header_value(line.trim(), "手合割").is_some();
        if is_game_header && saw_game_header {
            sections.push((std::mem::take(&mut current), current_start));
            current_start = index;
        }
        current.push_str(line);
        current.push('\n');
        saw_game_header |= is_game_header;
    }
    if !current.is_empty() {
        sections.push((current, current_start));
    }
    sections
}

fn header_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let remainder = line.strip_prefix(key)?;
    remainder
        .strip_prefix('：')
        .or_else(|| remainder.strip_prefix(':'))
        .map(str::trim)
}

fn terminal_marker(notation: &str) -> Option<&'static str> {
    let compact = compact_notation(notation);
    if compact.starts_with("投了") {
        Some("%TORYO")
    } else if compact.starts_with("中断") {
        Some("%CHUDAN")
    } else if compact.starts_with("千日手") {
        Some("%SENNICHITE")
    } else if compact.starts_with("持将棋") {
        Some("%JISHOGI")
    } else if compact.starts_with("詰み") {
        Some("%TSUMI")
    } else if compact.starts_with("切れ負け") {
        Some("%TIME_UP")
    } else if compact.starts_with("入玉勝ち") {
        Some("%KACHI")
    } else if compact.starts_with("反則負け") || compact.starts_with("反則勝ち") {
        Some("%ILLEGAL_MOVE")
    } else {
        None
    }
}

fn resolve_move(
    notation: &str,
    position: &PartialPosition,
    last_to: Option<Square>,
) -> Result<Move, String> {
    let mut compact = compact_notation(notation);
    let origin = parse_origin(&mut compact)?;
    let (to, rest) = if let Some(remainder) = compact.strip_prefix('同') {
        (
            last_to.ok_or_else(|| "KIF uses 同 before any move".to_owned())?,
            remainder,
        )
    } else {
        let mut chars = compact.chars();
        let file = chars
            .next()
            .and_then(fullwidth_file)
            .ok_or_else(|| "invalid KIF destination file".to_owned())?;
        let rank = chars
            .next()
            .and_then(kanji_rank)
            .ok_or_else(|| "invalid KIF destination rank".to_owned())?;
        (
            Square::new(file, rank).ok_or_else(|| "invalid KIF destination".to_owned())?,
            chars.as_str(),
        )
    };
    let (written_kind, suffix) =
        parse_written_piece(rest).ok_or_else(|| format!("unknown KIF piece notation: {rest}"))?;
    let is_drop = suffix.contains('打');
    let is_non_promoting = suffix.contains("不成");
    let is_promoting = !is_non_promoting && suffix.contains('成');
    let expected_kind = if is_promoting {
        written_kind
            .promote()
            .ok_or_else(|| "KIF requests promotion for an unpromotable piece".to_owned())?
    } else {
        written_kind
    };

    if let Some(from) = origin {
        if is_drop {
            return Err("KIF move cannot specify both an origin and a drop".to_owned());
        }
        let candidate = Move::Normal {
            from,
            to,
            promote: is_promoting,
        };
        if move_result_kind(position, candidate) != Some(expected_kind)
            || is_legal_partial(position, candidate).is_err()
        {
            return Err(format!("no legal move matches KIF notation: {notation}"));
        }
        return Ok(candidate);
    }
    if is_drop {
        if is_promoting || written_kind == PieceKind::King || written_kind.unpromote().is_some() {
            return Err(format!("invalid KIF drop notation: {notation}"));
        }
        let candidate = Move::Drop {
            piece: Piece::new(written_kind, position.side_to_move()),
            to,
        };
        if is_legal_partial(position, candidate).is_err() {
            return Err(format!("no legal move matches KIF notation: {notation}"));
        }
        return Ok(candidate);
    }

    let candidates: Vec<_> = all_legal_moves_partial(position)
        .into_iter()
        .filter(|candidate| candidate.to() == to)
        .filter(|candidate| origin.is_none_or(|value| candidate.from() == Some(value)))
        .filter(|candidate| !is_drop || candidate.is_drop())
        .filter(|candidate| is_drop || origin.is_some() || !candidate.is_drop())
        .filter(|candidate| move_result_kind(position, *candidate) == Some(expected_kind))
        .filter(|candidate| !is_non_promoting || !candidate.is_promoting())
        .collect();
    match candidates.as_slice() {
        [candidate] => Ok(*candidate),
        [] => Err(format!("no legal move matches KIF notation: {notation}")),
        _ => Err(format!(
            "ambiguous KIF notation matches {} legal moves: {notation}",
            candidates.len()
        )),
    }
}

fn parse_origin(compact: &mut String) -> Result<Option<Square>, String> {
    if !compact.ends_with(')') {
        return Ok(None);
    }
    let Some(open) = compact.rfind('(') else {
        return Err("KIF origin is missing an opening parenthesis".to_owned());
    };
    let digits = &compact[open + 1..compact.len() - 1];
    let bytes = digits.as_bytes();
    if bytes.len() != 2 || !bytes.iter().all(u8::is_ascii_digit) {
        return Err("invalid KIF origin".to_owned());
    }
    let square = Square::new(bytes[0] - b'0', bytes[1] - b'0')
        .ok_or_else(|| "invalid KIF origin square".to_owned())?;
    compact.truncate(open);
    Ok(Some(square))
}

fn parse_written_piece(text: &str) -> Option<(PieceKind, &str)> {
    let names = [
        ("成香", PieceKind::ProLance),
        ("成桂", PieceKind::ProKnight),
        ("成銀", PieceKind::ProSilver),
        ("歩", PieceKind::Pawn),
        ("香", PieceKind::Lance),
        ("桂", PieceKind::Knight),
        ("銀", PieceKind::Silver),
        ("金", PieceKind::Gold),
        ("角", PieceKind::Bishop),
        ("飛", PieceKind::Rook),
        ("玉", PieceKind::King),
        ("王", PieceKind::King),
        ("と", PieceKind::ProPawn),
        ("馬", PieceKind::ProBishop),
        ("龍", PieceKind::ProRook),
        ("竜", PieceKind::ProRook),
    ];
    names
        .into_iter()
        .find_map(|(name, kind)| text.strip_prefix(name).map(|suffix| (kind, suffix)))
}

fn move_result_kind(position: &PartialPosition, move_value: Move) -> Option<PieceKind> {
    match move_value {
        Move::Drop { piece, .. } => Some(piece.piece_kind()),
        Move::Normal { from, promote, .. } => {
            let kind = position.piece_at(from)?.piece_kind();
            if promote { kind.promote() } else { Some(kind) }
        }
    }
}

fn compact_notation(value: &str) -> String {
    value
        .chars()
        .filter(|value| !value.is_whitespace())
        .collect()
}

fn fullwidth_file(value: char) -> Option<u8> {
    Some(match value {
        '１' => 1,
        '２' => 2,
        '３' => 3,
        '４' => 4,
        '５' => 5,
        '６' => 6,
        '７' => 7,
        '８' => 8,
        '９' => 9,
        _ => return None,
    })
}

fn kanji_rank(value: char) -> Option<u8> {
    Some(match value {
        '一' => 1,
        '二' => 2,
        '三' => 3,
        '四' => 4,
        '五' => 5,
        '六' => 6,
        '七' => 7,
        '八' => 8,
        '九' => 9,
        _ => return None,
    })
}

fn decode_record(bytes: &[u8], source_path: &str) -> Result<String, RecordError> {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(text.to_owned());
    }
    let (text, _, had_errors) = SHIFT_JIS.decode(bytes);
    if had_errors {
        return Err(RecordError::new(
            source_path,
            RecordErrorKind::Decode,
            None,
            "record is neither valid UTF-8 nor decodable CP932",
        ));
    }
    Ok(text.into_owned())
}
