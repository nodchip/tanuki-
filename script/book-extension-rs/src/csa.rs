use encoding_rs::SHIFT_JIS;
use sha2::{Digest, Sha256};
use shogi_core::{Color, Hand, Move, PartialPosition, Piece, PieceKind, Square, ToUsi};
use shogi_legality_lite::is_legal_partial;

use crate::{
    STARTPOS_SFEN,
    record::{CorpusPosition, NormalizedGame, RecordError, RecordErrorKind},
};

struct ExplicitPosition {
    position: PartialPosition,
    dense_rows: [bool; 9],
    all_remaining: [bool; 2],
}

impl ExplicitPosition {
    fn new() -> Self {
        Self {
            position: PartialPosition::empty(),
            dense_rows: [false; 9],
            all_remaining: [false; 2],
        }
    }

    fn assign_remaining(&mut self, source_path: &str, line: usize) -> Result<(), RecordError> {
        if self.all_remaining == [true, true] {
            return Err(RecordError::new(
                source_path,
                RecordErrorKind::InvalidPositionToken,
                Some(line),
                "00AL cannot be assigned to both players",
            ));
        }
        for (index, color) in [Color::Black, Color::White].into_iter().enumerate() {
            if !self.all_remaining[index] {
                continue;
            }
            for (kind, total) in standard_inventory() {
                if kind == PieceKind::King {
                    continue;
                }
                let used = count_kind(&self.position, kind);
                if used > total {
                    return Err(RecordError::new(
                        source_path,
                        RecordErrorKind::InvalidPositionToken,
                        Some(line),
                        "position contains more pieces than the standard inventory",
                    ));
                }
                for _ in used..total {
                    let hand = self.position.hand_of_a_player_mut(color);
                    *hand = hand.added(kind).ok_or_else(|| {
                        RecordError::new(
                            source_path,
                            RecordErrorKind::InvalidPositionToken,
                            Some(line),
                            "00AL overflows a hand",
                        )
                    })?;
                }
            }
        }
        Ok(())
    }
}

pub fn csa_source_sha256(bytes: &[u8]) -> String {
    let normalized = decode_record(bytes, "<fingerprint>")
        .map(|text| text.replace("\r\n", "\n").replace('\r', "\n"))
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned());
    format!("{:x}", Sha256::digest(normalized.as_bytes()))
}
pub fn parse_csa(bytes: &[u8], source_path: &str) -> Result<NormalizedGame, RecordError> {
    let text = decode_record(bytes, source_path)?;
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut players = [String::new(), String::new()];
    let mut position: Option<PartialPosition> = None;
    let mut explicit: Option<ExplicitPosition> = None;
    let mut saw_pi = false;
    let mut turn_declared = false;
    let mut moves = Vec::new();
    let mut positions = Vec::new();
    let mut endgame = None;

    for (index, raw_line) in normalized.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with('V')
            || line.starts_with('$')
            || line.starts_with('\'')
        {
            continue;
        }
        if let Some(name) = line.strip_prefix("N+") {
            players[0] = name.to_owned();
            continue;
        }
        if let Some(name) = line.strip_prefix("N-") {
            players[1] = name.to_owned();
            continue;
        }
        if line == "PI" {
            if position.is_some() || explicit.is_some() || turn_declared {
                return Err(position_error(
                    source_path,
                    RecordErrorKind::InvalidPositionToken,
                    line_number,
                    "initial position is declared more than once",
                ));
            }
            position = Some(PartialPosition::startpos());
            saw_pi = true;
            continue;
        }
        if line.starts_with('P') {
            if saw_pi || turn_declared {
                return Err(position_error(
                    source_path,
                    RecordErrorKind::InvalidPositionToken,
                    line_number,
                    "position line appears after PI or turn declaration",
                ));
            }
            parse_position_line(
                raw_line,
                explicit.get_or_insert_with(ExplicitPosition::new),
                source_path,
                line_number,
            )?;
            continue;
        }
        if line == "+" || line == "-" {
            if position.is_none() {
                let Some(mut builder) = explicit.take() else {
                    return Err(RecordError::new(
                        source_path,
                        RecordErrorKind::TurnBeforePosition,
                        Some(line_number),
                        "turn line before initial position",
                    ));
                };
                builder.assign_remaining(source_path, line_number)?;
                position = Some(builder.position);
            }
            let current = position.as_mut().expect("position set above");
            let declared = if line == "+" {
                Color::Black
            } else {
                Color::White
            };
            current.side_to_move_set(declared);
            if canonical_sfen(&current.to_sfen_owned()) != canonical_sfen(STARTPOS_SFEN) {
                return Err(position_error(
                    source_path,
                    RecordErrorKind::NonStartpos,
                    line_number,
                    "game does not start from the standard even start position",
                ));
            }
            turn_declared = true;
            continue;
        }
        if line.starts_with('T') {
            if moves.is_empty() {
                return Err(RecordError::new(
                    source_path,
                    RecordErrorKind::TimeBeforeFirstMove,
                    Some(line_number),
                    "time line before first recorded move",
                ));
            }
            continue;
        }
        if line.starts_with("**") {
            if moves.is_empty() {
                return Err(RecordError::new(
                    source_path,
                    RecordErrorKind::ScoreBeforeFirstMove,
                    Some(line_number),
                    "score line before first recorded move",
                ));
            }
            continue;
        }
        if line.starts_with('%') {
            endgame = Some(strip_inline_time(line).to_owned());
            break;
        }
        if let Some(move_line) = csa_move_text(line) {
            let Some(current) = position.as_mut() else {
                return Err(RecordError::new(
                    source_path,
                    RecordErrorKind::TurnBeforePosition,
                    Some(line_number),
                    "move line before initial position",
                ));
            };
            if !turn_declared {
                return Err(RecordError::new(
                    source_path,
                    RecordErrorKind::TurnBeforePosition,
                    Some(line_number),
                    "move line before turn declaration",
                ));
            }
            let previous_sfen = current.to_sfen_owned();
            let move_value = parse_csa_move(move_line, current).map_err(|detail| {
                RecordError::new(
                    source_path,
                    RecordErrorKind::InvalidMove,
                    Some(line_number),
                    detail,
                )
                .with_position(previous_sfen.clone(), line)
            })?;
            if is_legal_partial(current, move_value).is_err() {
                return Err(RecordError::new(
                    source_path,
                    RecordErrorKind::IllegalMove,
                    Some(line_number),
                    "illegal or unreplayable CSA move",
                )
                .with_position(previous_sfen, line));
            }
            let move_usi = move_value.to_usi_owned();
            positions.push(CorpusPosition {
                sfen: current.to_sfen_owned(),
                move_usi: move_usi.clone(),
                ply: moves.len(),
            });
            current.make_move(move_value).ok_or_else(|| {
                RecordError::new(
                    source_path,
                    RecordErrorKind::IllegalMove,
                    Some(line_number),
                    "failed to apply legal CSA move",
                )
                .with_position(previous_sfen, line)
            })?;
            moves.push(move_usi);
            continue;
        }
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
            "game has no terminal marker",
        )
    })?;
    let sha256 = format!("{:x}", Sha256::digest(normalized.as_bytes()));
    Ok(NormalizedGame {
        source_path: source_path.to_owned(),
        sha256,
        players,
        moves,
        positions,
        endgame,
    })
}

fn parse_position_line(
    raw_line: &str,
    builder: &mut ExplicitPosition,
    source_path: &str,
    line_number: usize,
) -> Result<(), RecordError> {
    let bytes = raw_line.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'P' && matches!(bytes[1], b'1'..=b'9') {
        let rank = usize::from(bytes[1] - b'1');
        if builder.dense_rows[rank] {
            return Err(position_error(
                source_path,
                RecordErrorKind::InvalidPositionToken,
                line_number,
                "dense position row is duplicated",
            ));
        }
        let payload = &bytes[2..];
        if payload.len() != 27 {
            return Err(position_error(
                source_path,
                RecordErrorKind::InvalidPositionToken,
                line_number,
                "dense position row must contain exactly nine cells",
            ));
        }
        for index in 0..9 {
            let token = &payload[index * 3..index * 3 + 3];
            let square = Square::new(9 - index as u8, rank as u8 + 1).expect("bounded square");
            if token == b" * " {
                builder.position.piece_set(square, None);
                continue;
            }
            let color = match token[0] {
                b'+' => Color::Black,
                b'-' => Color::White,
                _ => return Err(invalid_position_token(source_path, line_number, token)),
            };
            let code = std::str::from_utf8(&token[1..]).unwrap_or("");
            let kind = piece_kind(code)
                .ok_or_else(|| invalid_position_token(source_path, line_number, token))?;
            if builder.position.piece_at(square).is_some() {
                return Err(position_error(
                    source_path,
                    RecordErrorKind::InvalidPositionToken,
                    line_number,
                    "position assigns one square more than once",
                ));
            }
            builder
                .position
                .piece_set(square, Some(Piece::new(kind, color)));
        }
        builder.dense_rows[rank] = true;
        return Ok(());
    }
    if bytes.len() >= 2 && bytes[0] == b'P' && matches!(bytes[1], b'+' | b'-') {
        let color = if bytes[1] == b'+' {
            Color::Black
        } else {
            Color::White
        };
        let color_index = if color == Color::Black { 0 } else { 1 };
        let payload = &bytes[2..];
        if payload.is_empty() || !payload.len().is_multiple_of(4) {
            return Err(position_error(
                source_path,
                RecordErrorKind::InvalidPositionToken,
                line_number,
                "sparse position line must contain four-byte tokens",
            ));
        }
        for token in payload.chunks_exact(4) {
            let code = std::str::from_utf8(&token[2..]).unwrap_or("");
            if token == b"00AL" {
                if builder.all_remaining[color_index] {
                    return Err(invalid_position_token(source_path, line_number, token));
                }
                builder.all_remaining[color_index] = true;
                continue;
            }
            let kind = piece_kind(code)
                .ok_or_else(|| invalid_position_token(source_path, line_number, token))?;
            if token[0..2] == *b"00" {
                if kind == PieceKind::King || kind.unpromote().is_some() {
                    return Err(invalid_position_token(source_path, line_number, token));
                }
                let hand: &mut Hand = builder.position.hand_of_a_player_mut(color);
                *hand = hand
                    .added(kind)
                    .ok_or_else(|| invalid_position_token(source_path, line_number, token))?;
                continue;
            }
            let square = square(token[0], token[1])
                .ok_or_else(|| invalid_position_token(source_path, line_number, token))?;
            if builder.position.piece_at(square).is_some() {
                return Err(position_error(
                    source_path,
                    RecordErrorKind::InvalidPositionToken,
                    line_number,
                    "position assigns one square more than once",
                ));
            }
            builder
                .position
                .piece_set(square, Some(Piece::new(kind, color)));
        }
        return Ok(());
    }
    Err(position_error(
        source_path,
        RecordErrorKind::InvalidPositionToken,
        line_number,
        "unknown CSA position line",
    ))
}

fn invalid_position_token(source_path: &str, line: usize, token: &[u8]) -> RecordError {
    position_error(
        source_path,
        RecordErrorKind::InvalidPositionToken,
        line,
        &format!(
            "invalid CSA position token: {}",
            String::from_utf8_lossy(token)
        ),
    )
}

fn position_error(
    source_path: &str,
    kind: RecordErrorKind,
    line: usize,
    detail: &str,
) -> RecordError {
    RecordError::new(source_path, kind, Some(line), detail)
}

fn count_kind(position: &PartialPosition, kind: PieceKind) -> u8 {
    let board_count = Square::all()
        .filter_map(|square| position.piece_at(square))
        .filter(|piece| piece.piece_kind().unpromote().unwrap_or(piece.piece_kind()) == kind)
        .count() as u8;
    let hand_count = [Color::Black, Color::White]
        .into_iter()
        .map(|color| position.hand(Piece::new(kind, color)).unwrap_or(0))
        .sum::<u8>();
    board_count.saturating_add(hand_count)
}

fn standard_inventory() -> [(PieceKind, u8); 8] {
    [
        (PieceKind::Pawn, 18),
        (PieceKind::Lance, 4),
        (PieceKind::Knight, 4),
        (PieceKind::Silver, 4),
        (PieceKind::Gold, 4),
        (PieceKind::Bishop, 2),
        (PieceKind::Rook, 2),
        (PieceKind::King, 2),
    ]
}

fn canonical_sfen(sfen: &str) -> &str {
    sfen.rsplit_once(' ').map_or(sfen, |(prefix, _)| prefix)
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

fn is_move_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() == 7
        && matches!(bytes[0], b'+' | b'-')
        && bytes[1..5].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_uppercase)
}

fn strip_inline_time(line: &str) -> &str {
    line.split_once(',').map_or(line, |(value, _)| value)
}
fn csa_move_text(line: &str) -> Option<&str> {
    let move_text = match line.split_once(',') {
        None => line,
        Some((move_text, suffix))
            if suffix.strip_prefix('T').is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|value| value.is_ascii_digit())
            }) =>
        {
            move_text
        }
        Some(_) => return None,
    };
    is_move_line(move_text).then_some(move_text)
}

fn parse_csa_move(line: &str, position: &PartialPosition) -> Result<Move, &'static str> {
    let bytes = line.as_bytes();
    let side = match bytes[0] {
        b'+' => Color::Black,
        b'-' => Color::White,
        _ => return Err("invalid CSA move side"),
    };
    if side != position.side_to_move() {
        return Err("CSA move side does not match side to move");
    }
    let to = square(bytes[3], bytes[4]).ok_or("invalid CSA destination")?;
    let result_kind = piece_kind(&line[5..7]).ok_or("unknown CSA piece code")?;
    if bytes[1] == b'0' && bytes[2] == b'0' {
        if result_kind == PieceKind::King || result_kind.unpromote().is_some() {
            return Err("promoted or king piece cannot be dropped");
        }
        return Ok(Move::Drop {
            piece: Piece::new(result_kind, side),
            to,
        });
    }
    let from = square(bytes[1], bytes[2]).ok_or("invalid CSA origin")?;
    let source_piece = position.piece_at(from).ok_or("CSA origin is empty")?;
    if source_piece.color() != side {
        return Err("CSA origin contains opponent piece");
    }
    let source_kind = source_piece.piece_kind();
    let promote = if result_kind == source_kind {
        false
    } else if source_kind.promote() == Some(result_kind) {
        true
    } else {
        return Err("CSA result piece does not match origin piece");
    };
    Ok(Move::Normal { from, to, promote })
}

fn square(file: u8, rank: u8) -> Option<Square> {
    Square::new(file.checked_sub(b'0')?, rank.checked_sub(b'0')?)
}

fn piece_kind(code: &str) -> Option<PieceKind> {
    Some(match code {
        "FU" => PieceKind::Pawn,
        "KY" => PieceKind::Lance,
        "KE" => PieceKind::Knight,
        "GI" => PieceKind::Silver,
        "KI" => PieceKind::Gold,
        "KA" => PieceKind::Bishop,
        "HI" => PieceKind::Rook,
        "OU" => PieceKind::King,
        "TO" => PieceKind::ProPawn,
        "NY" => PieceKind::ProLance,
        "NK" => PieceKind::ProKnight,
        "NG" => PieceKind::ProSilver,
        "UM" => PieceKind::ProBishop,
        "RY" => PieceKind::ProRook,
        _ => return None,
    })
}
