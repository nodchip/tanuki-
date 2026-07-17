use std::{
    fs::File,
    io::{self, BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    string::FromUtf8Error,
};

use thiserror::Error;

use crate::book::{BookEntry, BookParseError, OpeningBook, parse_book_entry_line, strip_sfen_ply};

const READER_CAPACITY: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum TargetBookError {
    #[error("target book I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("target book is not valid UTF-8: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("target book parse failed: {0}")]
    Parse(#[from] BookParseError),
    #[error("target book is not strictly sorted: previous={previous}, current={current}")]
    Unsorted { previous: String, current: String },
}

pub trait TargetBook: Send + Sync {
    fn lookup(&self, sfen: &str) -> Result<Vec<BookEntry>, TargetBookError>;
}

impl TargetBook for OpeningBook {
    fn lookup(&self, sfen: &str) -> Result<Vec<BookEntry>, TargetBookError> {
        Ok(self
            .position(sfen)
            .map(|position| position.entries.clone())
            .unwrap_or_default())
    }
}

#[derive(Clone, Debug)]
pub struct DiskOpeningBook {
    path: PathBuf,
    ignore_ply: bool,
}

impl DiskOpeningBook {
    pub fn open(path: &Path, ignore_ply: bool) -> Result<Self, TargetBookError> {
        let book = Self {
            path: path.to_owned(),
            ignore_ply,
        };
        book.validate_sorted()?;
        Ok(book)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn position_key<'a>(&self, sfen: &'a str) -> &'a str {
        if self.ignore_ply {
            strip_sfen_ply(sfen)
        } else {
            sfen
        }
    }

    fn validate_sorted(&self) -> Result<(), TargetBookError> {
        let file = File::open(&self.path)?;
        let mut reader = BufReader::with_capacity(READER_CAPACITY, file);
        let mut previous_key: Option<String> = None;
        let mut current_sfen: Option<String> = None;
        let mut entry_order = 0;
        while let Some(line) = read_text_line(&mut reader)? {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            if let Some(sfen) = line.strip_prefix("sfen ") {
                let sfen = sfen.trim();
                let key = self.position_key(sfen);
                if let Some(previous) = previous_key.as_deref()
                    && previous >= key
                {
                    return Err(TargetBookError::Unsorted {
                        previous: previous.to_owned(),
                        current: key.to_owned(),
                    });
                }
                previous_key = Some(key.to_owned());
                current_sfen = Some(sfen.to_owned());
                continue;
            }
            if current_sfen.is_none() {
                return Err(BookParseError::EntryBeforeSfen(line.to_owned()).into());
            }
            parse_book_entry_line(line, entry_order)?;
            entry_order += 1;
        }
        Ok(())
    }

    fn find_sfen_offset(
        &self,
        reader: &mut BufReader<File>,
        key: &str,
    ) -> Result<Option<u64>, TargetBookError> {
        let mut low = 0;
        let mut high = reader.get_ref().metadata()?.len();
        let mut candidate = None;
        while low < high {
            let mid = low + (high - low) / 2;
            let Some((offset, line)) = seek_next_sfen(reader, mid)? else {
                high = mid;
                continue;
            };
            let current_key = self.position_key(line[5..].trim());
            if current_key < key {
                low = reader.stream_position()?;
            } else {
                candidate = Some(offset);
                high = mid;
            }
        }
        Ok(candidate)
    }

    fn read_entries(
        &self,
        reader: &mut BufReader<File>,
    ) -> Result<Vec<BookEntry>, TargetBookError> {
        let mut entries = Vec::new();
        while let Some(line) = read_text_line(reader)? {
            let line = line.trim();
            if line.starts_with("sfen ") {
                break;
            }
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            entries.push(parse_book_entry_line(line, entries.len())?);
        }
        Ok(entries)
    }
}

impl TargetBook for DiskOpeningBook {
    fn lookup(&self, sfen: &str) -> Result<Vec<BookEntry>, TargetBookError> {
        let key = self.position_key(sfen);
        let file = File::open(&self.path)?;
        let mut reader = BufReader::with_capacity(READER_CAPACITY, file);
        let Some(offset) = self.find_sfen_offset(&mut reader, key)? else {
            return Ok(Vec::new());
        };
        reader.seek(SeekFrom::Start(offset))?;
        let Some(line) = read_text_line(&mut reader)? else {
            return Ok(Vec::new());
        };
        let Some(found_sfen) = line.trim().strip_prefix("sfen ") else {
            return Ok(Vec::new());
        };
        if self.position_key(found_sfen.trim()) != key {
            return Ok(Vec::new());
        }
        self.read_entries(&mut reader)
    }
}

fn seek_next_sfen(
    reader: &mut BufReader<File>,
    offset: u64,
) -> Result<Option<(u64, String)>, TargetBookError> {
    reader.seek(SeekFrom::Start(offset))?;
    if offset > 0 {
        let mut partial = Vec::new();
        reader.read_until(b'\n', &mut partial)?;
    }
    loop {
        let line_offset = reader.stream_position()?;
        let Some(line) = read_text_line(reader)? else {
            return Ok(None);
        };
        if line.trim().starts_with("sfen ") {
            return Ok(Some((line_offset, line)));
        }
    }
}

fn read_text_line(reader: &mut BufReader<File>) -> Result<Option<String>, TargetBookError> {
    let mut bytes = Vec::new();
    if reader.read_until(b'\n', &mut bytes)? == 0 {
        return Ok(None);
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    let text = String::from_utf8(bytes)?;
    Ok(Some(text.trim_start_matches('\u{feff}').to_owned()))
}
