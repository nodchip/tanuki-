use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::corpus_build_profile::{CorpusManifest, ManifestSource};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct DownloadSummary {
    pub downloaded: usize,
    pub reused: usize,
    pub bytes_downloaded: u64,
}

pub fn collect_manifest(
    manifest: &CorpusManifest,
    destination: &Path,
) -> Result<DownloadSummary, DownloadError> {
    collect_manifest_with_control(manifest, destination, || false)
}

pub fn collect_manifest_with_control<F>(
    manifest: &CorpusManifest,
    destination: &Path,
    mut should_stop: F,
) -> Result<DownloadSummary, DownloadError>
where
    F: FnMut() -> bool,
{
    std::fs::create_dir_all(destination).map_err(|source| DownloadError::Io {
        path: destination.to_path_buf(),
        source,
    })?;
    let mut summary = DownloadSummary::default();
    for (source_index, source) in manifest.sources.iter().enumerate() {
        if should_stop() {
            return Err(DownloadError::Stopped);
        }
        eprintln!(
            "[corpus] phase=download event=source_start source_index={} source_total={} input={} bytes_total={}",
            source_index + 1,
            manifest.sources.len(),
            source.relative_path,
            source.size
        );
        let target = destination.join(Path::new(&source.relative_path));
        if target.exists() {
            verify_file(&target, source)?;
            summary.reused += 1;
            eprintln!(
                "[corpus] phase=download event=source_done input={} reused=true bytes={}",
                source.relative_path, source.size
            );
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|source| DownloadError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let file_name = target
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| DownloadError::UnsafeTarget(source.relative_path.clone()))?;
        let partial = target.with_file_name(format!("{file_name}.part.{}", std::process::id()));
        if partial.exists() {
            std::fs::remove_file(&partial).map_err(|source| DownloadError::Io {
                path: partial.clone(),
                source,
            })?;
        }
        let result = download_source(source, &partial, &mut should_stop).and_then(|bytes| {
            std::fs::rename(&partial, &target).map_err(|source| DownloadError::Io {
                path: target.clone(),
                source,
            })?;
            Ok(bytes)
        });
        match result {
            Ok(bytes) => {
                summary.downloaded += 1;
                summary.bytes_downloaded += bytes;
            }
            Err(error) => {
                let _ = std::fs::remove_file(&partial);
                return Err(error);
            }
        }
    }
    let snapshot_path = destination.join("snapshot.json");
    let snapshot = Snapshot {
        manifest_version: manifest.manifest_version,
        created_at_utc: &manifest.created_at_utc,
        sources: &manifest.sources,
        summary,
    };
    let mut bytes = serde_json::to_vec_pretty(&snapshot)?;
    bytes.push(b'\n');
    std::fs::write(&snapshot_path, bytes).map_err(|source| DownloadError::Io {
        path: snapshot_path,
        source,
    })?;
    Ok(summary)
}

fn download_source<F>(
    source: &ManifestSource,
    partial: &Path,
    should_stop: &mut F,
) -> Result<u64, DownloadError>
where
    F: FnMut() -> bool,
{
    let mut response =
        ureq::get(&source.url)
            .call()
            .map_err(|source_error| DownloadError::Http {
                url: source.url.clone(),
                detail: source_error.to_string(),
            })?;
    let mut reader = response.body_mut().as_reader();
    let mut output = File::create(partial).map_err(|source| DownloadError::Io {
        path: partial.to_path_buf(),
        source,
    })?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let started = Instant::now();
    let mut next_progress = 16 * 1024 * 1024_u64;
    loop {
        if should_stop() {
            return Err(DownloadError::Stopped);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|source| DownloadError::Io {
                path: partial.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| DownloadError::SizeMismatch {
                path: source.relative_path.clone(),
                expected: source.size,
                actual: u64::MAX,
            })?;
        if total > source.size {
            return Err(DownloadError::SizeMismatch {
                path: source.relative_path.clone(),
                expected: source.size,
                actual: total,
            });
        }
        digest.update(&buffer[..read]);
        if total >= next_progress || total == source.size {
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let bytes_per_second = total as f64 / elapsed;
            let eta_seconds = if bytes_per_second > 0.0 {
                source.size.saturating_sub(total) as f64 / bytes_per_second
            } else {
                0.0
            };
            eprintln!(
                "[corpus] phase=download event=progress input={} bytes={} bytes_total={} percent={:.2} bytes_per_second={:.0} eta_seconds={:.1}",
                source.relative_path,
                total,
                source.size,
                100.0 * total as f64 / source.size.max(1) as f64,
                bytes_per_second,
                eta_seconds
            );
            next_progress = total.saturating_add(16 * 1024 * 1024);
        }
        output
            .write_all(&buffer[..read])
            .map_err(|source| DownloadError::Io {
                path: partial.to_path_buf(),
                source,
            })?;
    }
    output.sync_all().map_err(|source| DownloadError::Io {
        path: partial.to_path_buf(),
        source,
    })?;
    verify_values(source, total, &format!("{:x}", digest.finalize()))?;
    Ok(total)
}

fn verify_file(path: &Path, source: &ManifestSource) -> Result<(), DownloadError> {
    let mut file = File::open(path).map_err(|source| DownloadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|source| DownloadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        total += read as u64;
        digest.update(&buffer[..read]);
    }
    verify_values(source, total, &format!("{:x}", digest.finalize()))
}

fn verify_values(
    source: &ManifestSource,
    actual_size: u64,
    actual_hash: &str,
) -> Result<(), DownloadError> {
    if actual_size != source.size {
        return Err(DownloadError::SizeMismatch {
            path: source.relative_path.clone(),
            expected: source.size,
            actual: actual_size,
        });
    }
    if !actual_hash.eq_ignore_ascii_case(&source.sha256) {
        return Err(DownloadError::HashMismatch {
            path: source.relative_path.clone(),
            expected: source.sha256.clone(),
            actual: actual_hash.to_owned(),
        });
    }
    Ok(())
}

#[derive(Serialize)]
struct Snapshot<'a> {
    manifest_version: u32,
    created_at_utc: &'a str,
    sources: &'a [ManifestSource],
    summary: DownloadSummary,
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("download stopped by request")]
    Stopped,
    #[error("filesystem error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("HTTP request failed for {url}: {detail}")]
    Http { url: String, detail: String },
    #[error("download size mismatch for {path}: expected={expected}, actual={actual}")]
    SizeMismatch {
        path: String,
        expected: u64,
        actual: u64,
    },
    #[error("download hash mismatch for {path}: expected={expected}, actual={actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("unsafe download target: {0}")]
    UnsafeTarget(String),
    #[error("snapshot JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
