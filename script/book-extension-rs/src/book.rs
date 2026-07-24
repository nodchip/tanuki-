use std::{collections::HashMap, fs, path::Path};

use thiserror::Error;

pub const DEFAULT_HEADER: &str = "#YANEURAOU-DB2016 1.00";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookEntry {
    pub move_usi: String,
    pub response: String,
    pub eval_cp: i32,
    pub depth: i32,
    pub visits: u64,
    pub order_index: usize,
}

impl BookEntry {
    pub fn new(
        move_usi: impl Into<String>,
        response: impl Into<String>,
        eval_cp: i32,
        depth: i32,
        visits: u64,
        order_index: usize,
    ) -> Self {
        Self {
            move_usi: move_usi.into(),
            response: response.into(),
            eval_cp,
            depth,
            visits,
            order_index,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookPosition {
    pub sfen: String,
    pub entries: Vec<BookEntry>,
    pub order_index: usize,
}

impl BookPosition {
    pub fn find_entry(&self, move_usi: &str) -> Option<&BookEntry> {
        self.entries.iter().find(|entry| entry.move_usi == move_usi)
    }

    pub fn find_entry_mut(&mut self, move_usi: &str) -> Option<&mut BookEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.move_usi == move_usi)
    }
}

#[derive(Debug, Error)]
pub enum BookLoadError {
    #[error("book I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("book parse failed: {0}")]
    Parse(#[from] BookParseError),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum BookParseError {
    #[error("book entry appears before first sfen line: {0}")]
    EntryBeforeSfen(String),
    #[error("book entry has fewer than three fields: {0}")]
    TooFewFields(String),
    #[error("invalid integer in book entry: {line}")]
    InvalidInteger { line: String },
}

#[derive(Clone, Debug)]
pub struct OpeningBook {
    pub header: String,
    ignore_ply: bool,
    positions: Vec<BookPosition>,
    position_indexes: HashMap<String, usize>,
    next_entry_order: usize,
}

impl OpeningBook {
    pub fn new(ignore_ply: bool) -> Self {
        Self {
            header: DEFAULT_HEADER.to_owned(),
            ignore_ply,
            positions: Vec::new(),
            position_indexes: HashMap::new(),
            next_entry_order: 0,
        }
    }

    pub fn load(path: &Path, ignore_ply: bool) -> Result<Self, BookLoadError> {
        let text = fs::read_to_string(path)?;
        Ok(Self::from_text(&text, ignore_ply)?)
    }
    pub fn from_text(text: &str, ignore_ply: bool) -> Result<Self, BookParseError> {
        let mut book = Self::new(ignore_ply);
        let mut current_key: Option<String> = None;
        for (line_index, raw_line) in text.lines().enumerate() {
            let line = if line_index == 0 {
                raw_line.trim_start_matches('\u{feff}').trim()
            } else {
                raw_line.trim()
            };
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if line.starts_with('#') {
                if line.starts_with("#YANEURAOU-DB") {
                    book.header = line.to_owned();
                }
                continue;
            }
            if let Some(sfen) = line.strip_prefix("sfen ") {
                let sfen = sfen.trim();
                let key = book.position_key(sfen);
                book.ensure_position(sfen)?;
                current_key = Some(key);
                continue;
            }
            let key = current_key
                .as_deref()
                .ok_or_else(|| BookParseError::EntryBeforeSfen(line.to_owned()))?;
            let entry = parse_book_entry_line(line, book.next_entry_order)?;
            book.next_entry_order += 1;
            let position_index = book.position_indexes[key];
            book.positions[position_index].entries.push(entry);
        }
        Ok(book)
    }

    pub fn ensure_position(&mut self, sfen: &str) -> Result<&mut BookPosition, BookParseError> {
        let key = self.position_key(sfen);
        let index = match self.position_indexes.get(&key).copied() {
            Some(index) => index,
            None => {
                let index = self.positions.len();
                self.positions.push(BookPosition {
                    sfen: self.output_sfen(sfen),
                    entries: Vec::new(),
                    order_index: index,
                });
                self.position_indexes.insert(key, index);
                index
            }
        };
        Ok(&mut self.positions[index])
    }

    pub fn ensure_external_entry(
        &mut self,
        sfen: &str,
        source: &BookEntry,
    ) -> Result<String, BookParseError> {
        let key = self.position_key(sfen);
        let existing = self
            .position(&key)
            .and_then(|position| position.find_entry(&source.move_usi))
            .map(|entry| entry.move_usi.clone());
        if let Some(move_usi) = existing {
            let position = self.position_mut(&key).expect("position existed above");
            let entry = position
                .find_entry_mut(&move_usi)
                .expect("entry existed above");
            if entry.response.eq_ignore_ascii_case("none")
                && !source.response.eq_ignore_ascii_case("none")
            {
                entry.response.clone_from(&source.response);
            }
            return Ok(move_usi);
        }
        let order = self.next_entry_order;
        self.next_entry_order += 1;
        let position = self.ensure_position(sfen)?;
        position.entries.push(BookEntry::new(
            &source.move_usi,
            &source.response,
            source.eval_cp,
            source.depth,
            0,
            order,
        ));
        Ok(source.move_usi.clone())
    }
    pub fn position(&self, sfen: &str) -> Option<&BookPosition> {
        let key = self.position_key(sfen);
        self.position_indexes
            .get(&key)
            .and_then(|index| self.positions.get(*index))
    }

    pub fn position_mut(&mut self, sfen: &str) -> Option<&mut BookPosition> {
        let key = self.position_key(sfen);
        let index = self.position_indexes.get(&key).copied()?;
        self.positions.get_mut(index)
    }

    pub fn positions(&self) -> &[BookPosition] {
        &self.positions
    }

    pub fn positions_len(&self) -> usize {
        self.positions.len()
    }

    pub fn position_key(&self, sfen: &str) -> String {
        if self.ignore_ply {
            strip_sfen_ply(sfen).to_owned()
        } else {
            sfen.to_owned()
        }
    }

    pub fn output_sfen(&self, sfen: &str) -> String {
        if self.ignore_ply {
            replace_sfen_ply(sfen, "0")
        } else {
            sfen.to_owned()
        }
    }

    pub fn to_text(&self) -> String {
        let mut output = String::new();
        output.push_str(&self.header);
        output.push('\n');
        let mut positions: Vec<&BookPosition> = self.positions.iter().collect();
        positions.sort_by(|left, right| {
            let left_key = if self.ignore_ply {
                strip_sfen_ply(&left.sfen)
            } else {
                &left.sfen
            };
            let right_key = if self.ignore_ply {
                strip_sfen_ply(&right.sfen)
            } else {
                &right.sfen
            };
            left_key
                .cmp(right_key)
                .then_with(|| left.order_index.cmp(&right.order_index))
        });
        for position in positions {
            output.push_str("sfen ");
            output.push_str(&position.sfen);
            output.push('\n');
            let mut entries: Vec<&BookEntry> = position.entries.iter().collect();
            entries.sort_by_key(|entry| (std::cmp::Reverse(entry.eval_cp), entry.order_index));
            for entry in entries {
                output.push_str(&format!(
                    "{} {} {} {} {}\n",
                    entry.move_usi, entry.response, entry.eval_cp, entry.depth, entry.visits
                ));
            }
        }
        output
    }
}

pub fn parse_book_entry_line(line: &str, order_index: usize) -> Result<BookEntry, BookParseError> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 3 {
        return Err(BookParseError::TooFewFields(line.to_owned()));
    }
    let parse_i32 = |value: &str| {
        value
            .parse::<i32>()
            .map_err(|_| BookParseError::InvalidInteger {
                line: line.to_owned(),
            })
    };
    let eval_cp = parse_i32(fields[2])?;
    let depth = if fields.len() >= 4 {
        parse_i32(fields[3])?
    } else {
        0
    };
    let visits = if fields.len() >= 5 {
        fields[4]
            .parse::<u64>()
            .map_err(|_| BookParseError::InvalidInteger {
                line: line.to_owned(),
            })?
    } else {
        0
    };
    Ok(BookEntry::new(
        fields[0],
        fields[1],
        eval_cp,
        depth,
        visits,
        order_index,
    ))
}

pub fn strip_sfen_ply(sfen: &str) -> &str {
    match sfen.rsplit_once(' ') {
        Some((prefix, ply)) if ply.parse::<i64>().is_ok() => prefix,
        _ => sfen,
    }
}

pub fn replace_sfen_ply(sfen: &str, ply: &str) -> String {
    match sfen.rsplit_once(' ') {
        Some((prefix, old_ply)) if old_ply.parse::<i64>().is_ok() => {
            format!("{prefix} {ply}")
        }
        _ => format!("{sfen} {ply}"),
    }
}
