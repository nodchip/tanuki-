use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use shogi_core::{Color, Move, PartialPosition, Piece};
use shogi_legality_lite::is_legal_partial;
use shogi_usi_parser::FromUsi;

use crate::book::{BookEntry, BookParseError, OpeningBook, parse_book_entry_line};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IssueKind {
    InvalidSfen,
    NonIncreasingSfen,
    DuplicateMove,
    IllegalMove,
    IllegalResponse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationIssue {
    pub sfen: String,
    pub move_usi: Option<String>,
    pub kind: IssueKind,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationReport {
    pub positions: usize,
    pub entries: usize,
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn valid(&self) -> bool {
        self.issues.is_empty()
    }
}

impl IssueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSfen => "invalid-sfen",
            Self::NonIncreasingSfen => "non-increasing-sfen",
            Self::DuplicateMove => "duplicate-move",
            Self::IllegalMove => "illegal-move",
            Self::IllegalResponse => "illegal-response",
        }
    }
}

#[derive(Debug, Error)]
pub enum ValidationFileError {
    #[error("validation file I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("book parse failed: {0}")]
    Parse(#[from] BookParseError),
}

pub fn validate_file(path: &Path) -> Result<ValidationReport, ValidationFileError> {
    let file = File::open(path)?;
    let reader = BufReader::with_capacity(1024 * 1024, file);
    let mut report = ValidationReport {
        positions: 0,
        entries: 0,
        issues: Vec::new(),
    };
    let mut current_sfen: Option<String> = None;
    let mut previous_sfen: Option<String> = None;
    let mut current_position: Option<PartialPosition> = None;
    let mut seen = HashSet::new();
    let mut order_index = 0;
    for (line_index, line) in reader.lines().enumerate() {
        let line = line?;
        let line = if line_index == 0 {
            line.trim_start_matches('\u{feff}').trim()
        } else {
            line.trim()
        };
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if let Some(sfen) = line.strip_prefix("sfen ") {
            let sfen = sfen.trim().to_owned();
            if previous_sfen
                .as_ref()
                .is_some_and(|previous| previous >= &sfen)
            {
                report.issues.push(ValidationIssue {
                    sfen: sfen.clone(),
                    move_usi: None,
                    kind: IssueKind::NonIncreasingSfen,
                    detail: format!(
                        "SFEN groups must be strictly increasing; previous SFEN: {}",
                        previous_sfen.as_deref().unwrap_or_default()
                    ),
                });
            }
            previous_sfen = Some(sfen.clone());
            current_position = parse_position(&sfen);
            if current_position.is_none() {
                report.issues.push(ValidationIssue {
                    sfen: sfen.clone(),
                    move_usi: None,
                    kind: IssueKind::InvalidSfen,
                    detail: "SFEN cannot be parsed".to_owned(),
                });
            }
            current_sfen = Some(sfen);
            seen.clear();
            report.positions += 1;
            continue;
        }
        let sfen = current_sfen
            .as_deref()
            .ok_or_else(|| BookParseError::EntryBeforeSfen(line.to_owned()))?;
        let entry = parse_book_entry_line(line, order_index)?;
        order_index += 1;
        report.entries += 1;
        let Some(base) = current_position.as_ref() else {
            continue;
        };
        if seen.contains(&entry.move_usi) {
            report.issues.push(ValidationIssue {
                sfen: sfen.to_owned(),
                move_usi: Some(entry.move_usi),
                kind: IssueKind::DuplicateMove,
                detail: "move occurs more than once".to_owned(),
            });
            continue;
        }
        if let Some(issue) = entry_issue(sfen, base, &entry) {
            report.issues.push(issue);
            continue;
        }
        seen.insert(entry.move_usi);
    }
    Ok(report)
}

pub fn validate_book(book: &OpeningBook) -> ValidationReport {
    let mut report = ValidationReport {
        positions: book.positions_len(),
        entries: 0,
        issues: Vec::new(),
    };
    for position in book.positions() {
        report.entries += position.entries.len();
        let Some(base) = parse_position(&position.sfen) else {
            report.issues.push(ValidationIssue {
                sfen: position.sfen.clone(),
                move_usi: None,
                kind: IssueKind::InvalidSfen,
                detail: "SFEN cannot be parsed".to_owned(),
            });
            continue;
        };
        let mut seen = HashSet::new();
        for entry in &position.entries {
            if seen.contains(&entry.move_usi) {
                report.issues.push(ValidationIssue {
                    sfen: position.sfen.clone(),
                    move_usi: Some(entry.move_usi.clone()),
                    kind: IssueKind::DuplicateMove,
                    detail: "move occurs more than once".to_owned(),
                });
                continue;
            }
            if let Some(issue) = entry_issue(&position.sfen, &base, entry) {
                report.issues.push(issue);
                continue;
            }
            seen.insert(entry.move_usi.clone());
        }
    }
    report
}

pub fn legal_distinct_entry_count(sfen: &str, entries: &[BookEntry]) -> usize {
    let Some(base) = parse_position(sfen) else {
        return 0;
    };
    let mut seen = HashSet::new();
    entries
        .iter()
        .filter(|entry| {
            if seen.contains(&entry.move_usi) || entry_issue(sfen, &base, entry).is_some() {
                return false;
            }
            seen.insert(entry.move_usi.clone());
            true
        })
        .count()
}

pub(crate) fn entry_issue(
    sfen: &str,
    base: &PartialPosition,
    entry: &BookEntry,
) -> Option<ValidationIssue> {
    let Some(move_value) = parse_move(&entry.move_usi, base.side_to_move()) else {
        return Some(illegal_move_issue(sfen, entry));
    };
    if is_legal_partial(base, move_value).is_err() {
        return Some(illegal_move_issue(sfen, entry));
    }
    if entry.response.eq_ignore_ascii_case("none") {
        return None;
    }
    let mut next = base.clone();
    if next.make_move(move_value).is_none() {
        return Some(illegal_move_issue(sfen, entry));
    }
    let response = parse_move(&entry.response, next.side_to_move());
    if response
        .and_then(|value| is_legal_partial(&next, value).ok())
        .is_none()
    {
        return Some(ValidationIssue {
            sfen: sfen.to_owned(),
            move_usi: Some(entry.move_usi.clone()),
            kind: IssueKind::IllegalResponse,
            detail: format!("response {} is not legal after move", entry.response),
        });
    }
    None
}

fn illegal_move_issue(sfen: &str, entry: &BookEntry) -> ValidationIssue {
    ValidationIssue {
        sfen: sfen.to_owned(),
        move_usi: Some(entry.move_usi.clone()),
        kind: IssueKind::IllegalMove,
        detail: "move is not legal in position".to_owned(),
    }
}

pub fn parse_position(sfen: &str) -> Option<PartialPosition> {
    let normalized = match sfen.rsplit_once(' ') {
        Some((prefix, "0")) => format!("{prefix} 1"),
        _ => sfen.to_owned(),
    };
    PartialPosition::from_usi(&format!("sfen {normalized}")).ok()
}

pub(crate) fn parse_move(move_usi: &str, side: Color) -> Option<Move> {
    match Move::from_usi_lite(move_usi)? {
        Move::Drop { piece, to } => Some(Move::Drop {
            piece: Piece::new(piece.piece_kind(), side),
            to,
        }),
        move_value => Some(move_value),
    }
}
