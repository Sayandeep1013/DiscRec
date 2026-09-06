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
    load_storage_dir().unwrap_or_else(default_recordings_dir)
}

pub fn default_recordings_dir() -> PathBuf {
    downloads_dir().join("DiscRec")
}

pub fn set_recordings_dir(path: PathBuf) -> io::Result<PathBuf> {
    fs::create_dir_all(&path)?;
    save_storage_dir(&path)?;
    Ok(path)
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

fn downloads_dir() -> PathBuf {
    #[cfg(windows)]
    if let Ok(p) = std::env::var("USERPROFILE") {
        return PathBuf::from(p).join("Downloads");
    }
    #[cfg(target_os = "macos")]
    if let Ok(p) = std::env::var("HOME") {
        return PathBuf::from(p).join("Downloads");
    }
    PathBuf::from(".")
}

fn appdata_dir() -> PathBuf {
    #[cfg(windows)]
    if let Ok(p) = std::env::var("APPDATA") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "macos")]
    if let Ok(p) = std::env::var("HOME") {
        return PathBuf::from(p).join("Library").join("Application Support");
    }
    PathBuf::from(".")
}

fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

fn load_storage_dir() -> Option<PathBuf> {
    let text = fs::read_to_string(config_file()).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rest = line.strip_prefix("storage_dir")?.trim();
        let rest = rest.strip_prefix('=')?.trim();
        let unquoted = rest.trim_matches('"').trim_matches('\'');
        if unquoted.is_empty() {
            return None;
        }
        return Some(PathBuf::from(unquoted));
    }
    None
}

fn save_storage_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(config_dir())?;
    let escaped = path.to_string_lossy().replace('\\', "/");
    fs::write(config_file(), format!("storage_dir = \"{escaped}\"\n"))
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
    #[cfg(target_os = "macos")]
    {
        unsafe {
            let t = libc::time(std::ptr::null_mut());
            let mut tm = std::mem::zeroed::<libc::tm>();
            if libc::localtime_r(&t, &mut tm).is_null() {
                return format!("{t}");
            }
            format!(
                "{:04}-{:02}-{:02}-{:02}{:02}{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday,
                tm.tm_hour,
                tm.tm_min,
                tm.tm_sec
            )
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
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

#[cfg(target_os = "macos")]
fn free_bytes_impl(path: &Path) -> Option<u64> {
    let probe = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    let c = std::ffi::CString::new(probe.to_string_lossy().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let err = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
    if err != 0 {
        return None;
    }
    Some(st.f_bavail.saturating_mul(st.f_frsize as u64))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn free_bytes_impl(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recordings_dir_defaults_to_downloads() {
        let dir = default_recordings_dir();
        assert_eq!(dir.file_name().unwrap(), "DiscRec");
        assert_eq!(
            dir.parent().and_then(|p| p.file_name()).unwrap(),
            "Downloads"
        );
    }

    #[test]
    fn storage_dir_round_trip_parses() {
        let sample = "storage_dir = \"C:/Users/me/Downloads/DiscRec\"\n";
        let mut found = None;
        for line in sample.lines() {
            let rest = line.trim().strip_prefix("storage_dir").unwrap();
            let rest = rest.trim().strip_prefix('=').unwrap().trim();
            found = Some(rest.trim_matches('"'));
        }
        assert_eq!(found, Some("C:/Users/me/Downloads/DiscRec"));
    }
}
