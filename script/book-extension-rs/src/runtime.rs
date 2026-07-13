use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use thiserror::Error;

#[derive(Clone, Debug, PartialEq)]
pub struct RunStats {
    pub start_time: f64,
    pub added_positions: u64,
    pub searches: u64,
    pub total_nodes: u64,
}

impl RunStats {
    pub fn new(start_time: f64) -> Self {
        Self {
            start_time,
            added_positions: 0,
            searches: 0,
            total_nodes: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StopLimits {
    pub max_added_positions: Option<u64>,
    pub max_searches: Option<u64>,
    pub max_total_nodes: Option<u64>,
    pub max_runtime_sec: Option<f64>,
}

impl StopLimits {
    pub fn stop_reason(&self, stats: &RunStats, now: f64) -> Option<&'static str> {
        if self
            .max_added_positions
            .is_some_and(|limit| stats.added_positions >= limit)
        {
            return Some("max-added-positions");
        }
        if self
            .max_searches
            .is_some_and(|limit| stats.searches >= limit)
        {
            return Some("max-searches");
        }
        if self
            .max_total_nodes
            .is_some_and(|limit| stats.total_nodes >= limit)
        {
            return Some("max-total-nodes");
        }
        if self
            .max_runtime_sec
            .is_some_and(|limit| now - stats.start_time >= limit)
        {
            return Some("max-runtime-sec");
        }
        None
    }

    pub fn can_start_search(&self, stats: &RunStats, nodes: u64) -> bool {
        self.max_searches.is_none_or(|limit| stats.searches < limit)
            && self
                .max_total_nodes
                .is_none_or(|limit| stats.total_nodes.saturating_add(nodes) <= limit)
    }
}

#[derive(Clone, Debug)]
pub struct HeartbeatMonitor {
    path: PathBuf,
    timeout_sec: f64,
    started_at: f64,
    stop_request_path: Option<PathBuf>,
}

impl HeartbeatMonitor {
    pub fn new(
        path: &Path,
        timeout_sec: f64,
        started_at: f64,
        stop_request_path: Option<PathBuf>,
    ) -> Result<Self, RuntimeError> {
        if timeout_sec <= 0.0 {
            return Err(RuntimeError::InvalidTimeout);
        }
        Ok(Self {
            path: path.to_owned(),
            timeout_sec,
            started_at,
            stop_request_path,
        })
    }

    pub fn expired(&self, now: f64) -> bool {
        let last_seen = self
            .path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(self.started_at, |duration| duration.as_secs_f64());
        now - last_seen > self.timeout_sec
    }

    pub fn stop_reason(&self, now: f64) -> Option<&'static str> {
        if self
            .stop_request_path
            .as_ref()
            .is_some_and(|path| path.exists())
        {
            Some("manual-stop-request")
        } else if self.expired(now) {
            Some("jenkins-heartbeat-expired")
        } else {
            None
        }
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("timeout_sec must be positive")]
    InvalidTimeout,
    #[error("book lock is already held: {0}")]
    LockUnavailable(PathBuf),
    #[error("runtime I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

pub struct BookFileLock {
    file: File,
    #[allow(dead_code)]
    path: PathBuf,
}

impl BookFileLock {
    pub fn acquire(path: &Path, metadata: &str) -> Result<Self, RuntimeError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        lock_file(&file).map_err(|_| RuntimeError::LockUnavailable(path.to_owned()))?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        writeln!(file, "{metadata}")?;
        file.flush()?;
        file.sync_all()?;
        Ok(Self {
            file,
            path: path.to_owned(),
        })
    }
}

impl Drop for BookFileLock {
    fn drop(&mut self) {
        let _ = unlock_file(&self.file);
    }
}

#[cfg(windows)]
fn lock_file(file: &File) -> Result<(), std::io::Error> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx},
        System::IO::OVERLAPPED,
    };
    // SAFETY: OVERLAPPED is a plain Windows ABI structure. It is zero-initialized,
    // its offset fields are set before the synchronous LockFileEx call, and the
    // File handle remains owned by BookFileLock until the matching unlock/drop.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.Anonymous.Anonymous.Offset = i32::MAX as u32;
    let result = unsafe {
        LockFileEx(
            file.as_raw_handle() as HANDLE,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn unlock_file(file: &File) -> Result<(), std::io::Error> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::HANDLE, Storage::FileSystem::UnlockFileEx, System::IO::OVERLAPPED,
    };
    // SAFETY: This reconstructs the same byte range used by lock_file while the
    // original handle is still valid and exclusively owned by BookFileLock.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.Anonymous.Anonymous.Offset = i32::MAX as u32;
    let result = unsafe { UnlockFileEx(file.as_raw_handle() as HANDLE, 0, 1, 0, &mut overlapped) };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn lock_file(_file: &File) -> Result<(), std::io::Error> {
    Ok(())
}
#[cfg(not(windows))]
fn unlock_file(_file: &File) -> Result<(), std::io::Error> {
    Ok(())
}
