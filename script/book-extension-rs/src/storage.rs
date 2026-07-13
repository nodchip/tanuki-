use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use thiserror::Error;

use crate::{
    book::{BookParseError, OpeningBook},
    validation::{ValidationFileError, ValidationReport, validate_book, validate_file},
};

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("book validation failed")]
    Validation(ValidationReport),
    #[error("saved book cannot be parsed: {0}")]
    Parse(#[from] BookParseError),
    #[error("book I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("atomic persist failed: {0}")]
    Persist(#[from] tempfile::PersistError),
    #[error("saved book validation failed: {0}")]
    FileValidation(#[from] ValidationFileError),
}

pub fn save_validated_atomic(
    book: &OpeningBook,
    path: &Path,
    backup_count: usize,
) -> Result<ValidationReport, SaveError> {
    let report = validate_book(book);
    if !report.valid() {
        return Err(SaveError::Validation(report));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(book.to_text().as_bytes())?;
    temporary.as_file_mut().sync_all()?;

    let persisted_report = validate_file(temporary.path())?;
    if !persisted_report.valid() {
        return Err(SaveError::Validation(persisted_report));
    }

    rotate_backups(path, backup_count)?;
    temporary.persist(path)?;
    Ok(persisted_report)
}

pub fn backup_path(path: &Path, index: usize) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{name}.{index:03}.bak"))
}

fn rotate_backups(path: &Path, backup_count: usize) -> Result<(), std::io::Error> {
    if backup_count == 0 || !path.exists() {
        return Ok(());
    }
    for index in (1..backup_count).rev() {
        let source = backup_path(path, index);
        let destination = backup_path(path, index + 1);
        if source.exists() {
            if destination.exists() {
                fs::remove_file(&destination)?;
            }
            fs::rename(source, destination)?;
        }
    }
    let first = backup_path(path, 1);
    if first.exists() {
        fs::remove_file(&first)?;
    }
    fs::copy(path, first)?;
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
