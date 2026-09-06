//! Storage locations and disk-space gates (R13 defaults, desktop-shell).
//!
//! No config file is required. Paths follow the OS conventions in
//! `docs/spec/configuration.md`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Refuse to start a recording under this many free bytes.
pub const REFUSE_FREE_BYTES: u64 = 500 * 1024 * 1024;
/// Warn in the UI under this many free bytes.
pub const WARN_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

const _: () = assert!(REFUSE_FREE_BYTES < WARN_FREE_BYTES);

pub fn recordings_dir() -> PathBuf {
    documents_dir().join("DiscRec")
}

pub fn config_dir() -> PathBuf {
    appdata_dir().join("DiscRec")
}

pub fn notice_seen_path() -> PathBuf {
    config_dir().join("notice_seen")
}

pub fn notice_was_shown() -> bool {
    notice_seen_path().is_file()
}

pub fn mark_notice_shown() {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(notice_seen_path(), b"1\n");
}

pub fn ensure_recordings_dir() -> io::Result<PathBuf> {
    let dir = recordings_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// `DiscRec-YYYY-MM-DD-HHMMSS.ogg` in the recordings folder.
pub fn new_recording_path() -> io::Result<PathBuf> {
    let dir = ensure_recordings_dir()?;
    Ok(dir.join(format!("DiscRec-{}.ogg", timestamp_stamp())))
}

pub fn free_bytes_on(path: &Path) -> Option<u64> {
    free_bytes_impl(path)
}

pub fn disk_status(path: &Path) -> DiskStatus {
    match free_bytes_on(path) {
        Some(b) if b < REFUSE_FREE_BYTES => DiskStatus::Refuse,
        Some(b) if b < WARN_FREE_BYTES => DiskStatus::Warn,
        Some(_) => DiskStatus::Ok,
        None => DiskStatus::Ok, // if we cannot measure, do not block Record
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DiskStatus {
    Ok,
    Warn,
    Refuse,
}

fn documents_dir() -> PathBuf {
    if let Ok(p) = std::env::var("USERPROFILE") {
        return PathBuf::from(p).join("Documents");
    }
    PathBuf::from(".")
}

fn appdata_dir() -> PathBuf {
    if let Ok(p) = std::env::var("APPDATA") {
        return PathBuf::from(p);
    }
    PathBuf::from(".")
}

fn timestamp_stamp() -> String {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::SYSTEMTIME;
        let st: SYSTEMTIME = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        format!(
            "{:04}-{:02}-{:02}-{:02}{:02}{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        )
    }
    #[cfg(not(windows))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("{secs}")
    }
}

#[cfg(windows)]
fn free_bytes_impl(path: &Path) -> Option<u64> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let probe = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    let wide = HSTRING::from(probe.as_os_str());
    let mut free: u64 = 0;
    unsafe {
        GetDiskFreeSpaceExW(&wide, Some(&mut free), None, None).ok()?;
    }
    Some(free)
}

#[cfg(not(windows))]
fn free_bytes_impl(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recordings_dir_ends_with_discrec() {
        let dir = recordings_dir();
        assert_eq!(dir.file_name().unwrap(), "DiscRec");
    }
}
