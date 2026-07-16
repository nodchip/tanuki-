use std::{
    cell::Cell,
    collections::HashSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SevenZipInfo {
    pub path: PathBuf,
    pub version: String,
    pub sha256: String,
}

pub fn inspect_seven_zip(explicit: Option<&Path>) -> Result<SevenZipInfo, SourceError> {
    let path = resolve_seven_zip(explicit).ok_or(SourceError::SevenZipRequired)?;
    let version = validate_seven_zip(&path)?;
    let mut file = File::open(&path).map_err(|source| SourceError::Io {
        path: path.clone(),
        source,
    })?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|source| SourceError::Io {
            path: path.clone(),
            source,
        })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(SevenZipInfo {
        path,
        version,
        sha256: format!("{:x}", digest.finalize()),
    })
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct SourceVisitStats {
    pub record_members: u64,
    pub filtered_members: u64,
    pub visited_records: u64,
}

struct MemberFilter {
    pattern: Option<Regex>,
    record_members: Cell<u64>,
    filtered_members: Cell<u64>,
    visited_records: Cell<u64>,
}

impl MemberFilter {
    fn new(pattern: Option<&str>) -> Result<Self, regex::Error> {
        Ok(Self {
            pattern: pattern.map(Regex::new).transpose()?,
            record_members: Cell::new(0),
            filtered_members: Cell::new(0),
            visited_records: Cell::new(0),
        })
    }

    fn accepts(&self, path: &str) -> bool {
        self.record_members.set(self.record_members.get() + 1);
        let accepted = self
            .pattern
            .as_ref()
            .is_none_or(|value| value.is_match(path));
        if !accepted {
            self.filtered_members.set(self.filtered_members.get() + 1);
        }
        accepted
    }

    fn record_visited(&self) {
        self.visited_records.set(self.visited_records.get() + 1);
    }

    fn stats(&self) -> SourceVisitStats {
        SourceVisitStats {
            record_members: self.record_members.get(),
            filtered_members: self.filtered_members.get(),
            visited_records: self.visited_records.get(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordFormat {
    Csa,
    Kif,
}

impl RecordFormat {
    fn from_path(path: &str) -> Option<Self> {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".csa") {
            Some(Self::Csa)
        } else if lower.ends_with(".kif") || lower.ends_with(".kifu") {
            Some(Self::Kif)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ArchiveLimits {
    pub max_members: usize,
    pub max_member_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_members: 5_000_000,
            max_member_bytes: 64 * 1024 * 1024,
            max_total_bytes: 256 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRecord {
    pub relative_path: String,
    pub format: RecordFormat,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("source I/O failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("ZIP read failed: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("invalid member filter: {0}")]
    Regex(#[from] regex::Error),
    #[error("unsupported corpus source: {0}")]
    Unsupported(String),
    #[error("unsafe archive member path: {0}")]
    UnsafeMemberPath(String),
    #[error("duplicate archive member path: {0}")]
    DuplicateMemberPath(String),
    #[error("encrypted archive member is not supported: {0}")]
    EncryptedMember(String),
    #[error("7-Zip executable is required for this source")]
    SevenZipRequired,
    #[error("7-Zip command failed during {operation}: {detail}")]
    SevenZipCommand {
        operation: &'static str,
        detail: String,
    },
    #[error("archive contains more than {limit} record members")]
    TooManyMembers { limit: usize },
    #[error("archive member {path} has {size} bytes, limit is {limit}")]
    MemberTooLarge { path: String, size: u64, limit: u64 },
    #[error("archive record bytes {size} exceed total limit {limit}")]
    TotalTooLarge { size: u64, limit: u64 },
}

#[derive(Debug)]
pub enum VisitRecordsError<E> {
    Source(SourceError),
    Visitor(E),
}

impl<E> From<SourceError> for VisitRecordsError<E> {
    fn from(value: SourceError) -> Self {
        Self::Source(value)
    }
}

impl<E> From<zip::result::ZipError> for VisitRecordsError<E> {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Source(SourceError::Zip(value))
    }
}

impl<E> From<regex::Error> for VisitRecordsError<E> {
    fn from(value: regex::Error) -> Self {
        Self::Source(SourceError::Regex(value))
    }
}

pub fn read_records(
    path: &Path,
    member_pattern: Option<&str>,
    limits: ArchiveLimits,
    seven_zip: Option<&Path>,
) -> Result<Vec<SourceRecord>, SourceError> {
    let mut records = Vec::new();
    match visit_records(path, member_pattern, limits, seven_zip, |record| {
        records.push(record);
        Ok::<_, std::convert::Infallible>(())
    }) {
        Ok(_) => Ok(records),
        Err(VisitRecordsError::Source(error)) => Err(error),
        Err(VisitRecordsError::Visitor(never)) => match never {},
    }
}

pub fn visit_records<E, F>(
    path: &Path,
    member_pattern: Option<&str>,
    limits: ArchiveLimits,
    seven_zip: Option<&Path>,
    mut visitor: F,
) -> Result<SourceVisitStats, VisitRecordsError<E>>
where
    F: FnMut(SourceRecord) -> Result<(), E>,
{
    let filter = MemberFilter::new(member_pattern)?;
    let mut counted_visitor = |record| {
        filter.record_visited();
        visitor(record)
    };
    let lower = path.to_string_lossy().to_ascii_lowercase();
    let result = if path.is_dir() {
        visit_directory(path, &filter, limits, &mut counted_visitor)
    } else if lower.ends_with(".tar.xz") {
        visit_tar_xz(path, &filter, limits, &mut counted_visitor)
    } else if lower.ends_with(".zip") {
        visit_zip(path, &filter, limits, &mut counted_visitor)
    } else if lower.ends_with(".7z") || lower.ends_with(".lzh") || lower.ends_with(".lha") {
        visit_external_archive(path, &filter, limits, seven_zip, &mut counted_visitor)
    } else if RecordFormat::from_path(&lower).is_some() {
        visit_loose(path, &filter, limits, &mut counted_visitor)
    } else {
        Err(SourceError::Unsupported(path.display().to_string()).into())
    };
    result?;
    Ok(filter.stats())
}
fn visit_loose<E, F>(
    path: &Path,
    filter: &MemberFilter,
    limits: ArchiveLimits,
    visitor: &mut F,
) -> Result<(), VisitRecordsError<E>>
where
    F: FnMut(SourceRecord) -> Result<(), E>,
{
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| SourceError::UnsafeMemberPath(path.display().to_string()))?;
    let normalized = normalize_member_path(name)?;
    if !filter.accepts(&normalized) {
        return Ok(());
    }
    let size = path
        .metadata()
        .map_err(|source| SourceError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len();
    check_sizes(&normalized, size, size, 1, limits)?;
    let bytes = std::fs::read(path).map_err(|source| SourceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    visitor(SourceRecord {
        relative_path: normalized.clone(),
        format: RecordFormat::from_path(&normalized)
            .ok_or_else(|| SourceError::Unsupported(normalized.clone()))?,
        bytes,
    })
    .map_err(VisitRecordsError::Visitor)
}

fn visit_zip<E, F>(
    path: &Path,
    filter: &MemberFilter,
    limits: ArchiveLimits,
    visitor: &mut F,
) -> Result<(), VisitRecordsError<E>>
where
    F: FnMut(SourceRecord) -> Result<(), E>,
{
    let file = File::open(path).map_err(|source| SourceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive = ZipArchive::new(file)?;
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let normalized = normalize_member_path(entry.name())?;
        let Some(format) = RecordFormat::from_path(&normalized) else {
            continue;
        };
        if !filter.accepts(&normalized) {
            continue;
        }
        if entry.encrypted() {
            return Err(SourceError::EncryptedMember(normalized).into());
        }
        if !seen.insert(normalized.clone()) {
            return Err(SourceError::DuplicateMemberPath(normalized).into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or(SourceError::TotalTooLarge {
                size: u64::MAX,
                limit: limits.max_total_bytes,
            })?;
        check_sizes(&normalized, entry.size(), total, selected.len() + 1, limits)?;
        selected.push((normalized, index, entry.size(), format));
    }
    selected.sort_by(|left, right| left.0.cmp(&right.0));

    let archive_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| SourceError::UnsafeMemberPath(path.display().to_string()))?;
    for (member_path, index, size, format) in selected {
        let mut entry = archive.by_index(index)?;
        let capacity = usize::try_from(size).unwrap_or(0);
        let mut bytes = Vec::with_capacity(capacity);
        entry
            .by_ref()
            .take(limits.max_member_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|source| SourceError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if bytes.len() as u64 != size {
            return Err(SourceError::MemberTooLarge {
                path: member_path,
                size: bytes.len() as u64,
                limit: limits.max_member_bytes,
            }
            .into());
        }
        visitor(SourceRecord {
            relative_path: format!("{archive_name}!/{member_path}"),
            format,
            bytes,
        })
        .map_err(VisitRecordsError::Visitor)?;
    }
    Ok(())
}

fn visit_directory<E, F>(
    root: &Path,
    filter: &MemberFilter,
    limits: ArchiveLimits,
    visitor: &mut F,
) -> Result<(), VisitRecordsError<E>>
where
    F: FnMut(SourceRecord) -> Result<(), E>,
{
    let mut paths = Vec::new();
    collect_directory_files(root, root, &mut paths)?;
    paths.sort_by(|left, right| left.0.cmp(&right.0));
    let mut total = 0_u64;
    let mut count = 0_usize;
    for (relative, path) in paths {
        let Some(format) = RecordFormat::from_path(&relative) else {
            continue;
        };
        if !filter.accepts(&relative) {
            continue;
        }
        let size = path
            .metadata()
            .map_err(|source| SourceError::Io {
                path: path.clone(),
                source,
            })?
            .len();
        total = total.checked_add(size).ok_or(SourceError::TotalTooLarge {
            size: u64::MAX,
            limit: limits.max_total_bytes,
        })?;
        count += 1;
        check_sizes(&relative, size, total, count, limits)?;
        let bytes = std::fs::read(&path).map_err(|source| SourceError::Io {
            path: path.clone(),
            source,
        })?;
        visitor(SourceRecord {
            relative_path: relative,
            format,
            bytes,
        })
        .map_err(VisitRecordsError::Visitor)?;
    }
    Ok(())
}

fn collect_directory_files(
    root: &Path,
    directory: &Path,
    output: &mut Vec<(String, PathBuf)>,
) -> Result<(), SourceError> {
    let entries = std::fs::read_dir(directory).map_err(|source| SourceError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| SourceError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|source| SourceError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(SourceError::UnsafeMemberPath(path.display().to_string()));
        }
        if metadata.is_dir() {
            collect_directory_files(root, &path, output)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| SourceError::UnsafeMemberPath(path.display().to_string()))?
                .to_str()
                .ok_or_else(|| SourceError::UnsafeMemberPath(path.display().to_string()))?;
            output.push((normalize_member_path(relative)?, path));
        }
    }
    Ok(())
}

fn visit_tar_xz<E, F>(
    path: &Path,
    filter: &MemberFilter,
    limits: ArchiveLimits,
    visitor: &mut F,
) -> Result<(), VisitRecordsError<E>>
where
    F: FnMut(SourceRecord) -> Result<(), E>,
{
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let spool = tempfile::Builder::new()
        .prefix("corpus-tar-spool-")
        .tempdir_in(parent)
        .map_err(|source| SourceError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    let source = File::open(path).map_err(|source| SourceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let decoder = xz2::read::XzDecoder::new(source);
    let mut archive = tar::Archive::new(decoder);
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0_u64;
    let entries = archive.entries().map_err(|source| SourceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let mut entry = entry.map_err(|source| SourceError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let member = entry.path().map_err(|source| SourceError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let member = member
            .to_str()
            .ok_or_else(|| SourceError::UnsafeMemberPath(member.display().to_string()))?;
        let normalized = normalize_member_path(member)?;
        let Some(format) = RecordFormat::from_path(&normalized) else {
            continue;
        };
        if !filter.accepts(&normalized) {
            continue;
        }
        if !seen.insert(normalized.clone()) {
            return Err(SourceError::DuplicateMemberPath(normalized).into());
        }
        let size = entry.size();
        total = total.checked_add(size).ok_or(SourceError::TotalTooLarge {
            size: u64::MAX,
            limit: limits.max_total_bytes,
        })?;
        check_sizes(&normalized, size, total, selected.len() + 1, limits)?;
        let target = spool.path().join(Path::new(&normalized));
        if let Some(target_parent) = target.parent() {
            std::fs::create_dir_all(target_parent).map_err(|source| SourceError::Io {
                path: target_parent.to_path_buf(),
                source,
            })?;
        }
        let mut output = File::create(&target).map_err(|source| SourceError::Io {
            path: target.clone(),
            source,
        })?;
        let copied = std::io::copy(
            &mut entry
                .by_ref()
                .take(limits.max_member_bytes.saturating_add(1)),
            &mut output,
        )
        .map_err(|source| SourceError::Io {
            path: target.clone(),
            source,
        })?;
        if copied != size {
            return Err(SourceError::MemberTooLarge {
                path: normalized,
                size: copied,
                limit: limits.max_member_bytes,
            }
            .into());
        }
        selected.push((normalized, target, format));
    }
    selected.sort_by(|left, right| left.0.cmp(&right.0));
    let archive_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| SourceError::UnsafeMemberPath(path.display().to_string()))?;
    for (member, target, format) in selected {
        let bytes = std::fs::read(&target).map_err(|source| SourceError::Io {
            path: target,
            source,
        })?;
        visitor(SourceRecord {
            relative_path: format!("{archive_name}!/{member}"),
            format,
            bytes,
        })
        .map_err(VisitRecordsError::Visitor)?;
    }
    Ok(())
}
fn visit_external_archive<E, F>(
    path: &Path,
    filter: &MemberFilter,
    limits: ArchiveLimits,
    explicit_executable: Option<&Path>,
    visitor: &mut F,
) -> Result<(), VisitRecordsError<E>>
where
    F: FnMut(SourceRecord) -> Result<(), E>,
{
    let executable = resolve_seven_zip(explicit_executable).ok_or(SourceError::SevenZipRequired)?;
    let _version = validate_seven_zip(&executable)?;
    let listing = std::process::Command::new(&executable)
        .args(["l", "-slt", "-ba", "-sccUTF-8"])
        .arg(path)
        .output()
        .map_err(|source| SourceError::Io {
            path: executable.clone(),
            source,
        })?;
    if !listing.status.success() {
        return Err(command_error("list", &listing).into());
    }
    let listing = String::from_utf8_lossy(&listing.stdout).replace('\r', "");
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0_u64;
    let mut file_count = 0_usize;
    for block in listing.split("\n\n") {
        let mut member_path = None;
        let mut folder = false;
        let mut size = None;
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("Path = ") {
                member_path = Some(value.to_owned());
            } else if let Some(value) = line.strip_prefix("Folder = ") {
                folder = value.trim() == "+";
            } else if let Some(value) = line.strip_prefix("Size = ") {
                size = value.trim().parse::<u64>().ok();
            }
        }
        let Some(raw_path) = member_path else {
            continue;
        };
        if folder && raw_path == "." {
            continue;
        }
        let normalized = normalize_member_path(&raw_path)?;
        if !seen.insert(normalized.clone()) {
            return Err(SourceError::DuplicateMemberPath(normalized).into());
        }
        if folder {
            continue;
        }
        let size = size.ok_or_else(|| SourceError::SevenZipCommand {
            operation: "list",
            detail: format!("file entry has no size: {normalized}"),
        })?;
        file_count += 1;
        total = total.checked_add(size).ok_or(SourceError::TotalTooLarge {
            size: u64::MAX,
            limit: limits.max_total_bytes,
        })?;
        check_sizes(&normalized, size, total, file_count, limits)?;
        let Some(format) = RecordFormat::from_path(&normalized) else {
            continue;
        };
        if !filter.accepts(&normalized) {
            continue;
        }
        selected.push((normalized, size, format));
    }
    selected.sort_by(|left, right| left.0.cmp(&right.0));

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let spool = tempfile::Builder::new()
        .prefix("corpus-7z-spool-")
        .tempdir_in(parent)
        .map_err(|source| SourceError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    let output_flag = format!("-o{}", spool.path().display());
    let extraction = std::process::Command::new(&executable)
        .arg("x")
        .arg(path)
        .args([output_flag.as_str(), "-y", "-bb0", "-sccUTF-8"])
        .output()
        .map_err(|source| SourceError::Io {
            path: executable,
            source,
        })?;
    if !extraction.status.success() {
        return Err(command_error("extract", &extraction).into());
    }
    let spool_root = spool
        .path()
        .canonicalize()
        .map_err(|source| SourceError::Io {
            path: spool.path().to_path_buf(),
            source,
        })?;
    let archive_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| SourceError::UnsafeMemberPath(path.display().to_string()))?;
    for (member, size, format) in selected {
        let target = spool.path().join(Path::new(&member));
        let metadata = std::fs::symlink_metadata(&target).map_err(|source| SourceError::Io {
            path: target.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != size {
            return Err(SourceError::UnsafeMemberPath(member).into());
        }
        let canonical = target.canonicalize().map_err(|source| SourceError::Io {
            path: target.clone(),
            source,
        })?;
        if !canonical.starts_with(&spool_root) {
            return Err(SourceError::UnsafeMemberPath(member).into());
        }
        let bytes = std::fs::read(&canonical).map_err(|source| SourceError::Io {
            path: canonical,
            source,
        })?;
        visitor(SourceRecord {
            relative_path: format!("{archive_name}!/{member}"),
            format,
            bytes,
        })
        .map_err(VisitRecordsError::Visitor)?;
    }
    Ok(())
}

fn resolve_seven_zip(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return path.is_file().then(|| path.to_path_buf());
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            let candidate = directory.join("7z.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let standard = PathBuf::from(r"C:\Program Files\7-Zip\7z.exe");
    standard.is_file().then_some(standard)
}

fn validate_seven_zip(executable: &Path) -> Result<String, SourceError> {
    let output = std::process::Command::new(executable)
        .args(["i", "-sccUTF-8"])
        .output()
        .map_err(|source| SourceError::Io {
            path: executable.to_path_buf(),
            source,
        })?;
    if !output.status.success() || !String::from_utf8_lossy(&output.stdout).contains("7-Zip") {
        return Err(command_error("version", &output));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .lines()
        .find(|line| line.contains("7-Zip"))
        .unwrap_or("7-Zip (version unknown)")
        .trim()
        .to_owned();
    Ok(version)
}

fn command_error(operation: &'static str, output: &std::process::Output) -> SourceError {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    SourceError::SevenZipCommand {
        operation,
        detail: detail.chars().take(2000).collect(),
    }
}
fn normalize_member_path(value: &str) -> Result<String, SourceError> {
    let replaced = value.replace('\\', "/");
    let mut normalized = replaced.as_str();
    while let Some(remainder) = normalized.strip_prefix("./") {
        normalized = remainder;
    }
    if normalized.starts_with('/') || normalized.contains(':') {
        return Err(SourceError::UnsafeMemberPath(value.to_owned()));
    }
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(SourceError::UnsafeMemberPath(value.to_owned()));
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return Err(SourceError::UnsafeMemberPath(value.to_owned()));
    }
    Ok(parts.join("/"))
}

fn check_sizes(
    path: &str,
    member_size: u64,
    total_size: u64,
    members: usize,
    limits: ArchiveLimits,
) -> Result<(), SourceError> {
    if members > limits.max_members {
        return Err(SourceError::TooManyMembers {
            limit: limits.max_members,
        });
    }
    if member_size > limits.max_member_bytes {
        return Err(SourceError::MemberTooLarge {
            path: path.to_owned(),
            size: member_size,
            limit: limits.max_member_bytes,
        });
    }
    if total_size > limits.max_total_bytes {
        return Err(SourceError::TotalTooLarge {
            size: total_size,
            limit: limits.max_total_bytes,
        });
    }
    Ok(())
}
