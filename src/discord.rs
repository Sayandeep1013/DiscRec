//! Finding Discord's process to attach to.
//!
//! Not "detect that a call started" — auto-start was removed
//! (`docs/adr/0008-manual-control.md`). This is a process lookup.

/// Executable names to look for, in preference order.
#[cfg(windows)]
pub const DISCORD_PROCESSES: &[&str] = &["Discord.exe", "DiscordCanary.exe", "DiscordPTB.exe"];

#[cfg(target_os = "macos")]
pub const DISCORD_PROCESSES: &[&str] = &["Discord", "Discord Canary", "Discord PTB"];

/// A Discord process found on the system.
#[derive(Debug, Clone, Copy)]
pub struct Found {
    pub pid: u32,
    /// Index into `DISCORD_PROCESSES` — lower is more preferred.
    pub variant: usize,
}

/// Locate Discord's **root** process.
///
/// Discord runs a tree: a main process plus renderer, GPU and utility children,
/// all named the same. Audio is rendered by a child, so we return the root and
/// let the capture backend attach with `INCLUDE_TARGET_PROCESS_TREE`.
/// Targeting a leaf captures nothing while still reporting success — see
/// `docs/05-challenges.md`, P2.
///
/// Never matches on window title: it changes with the active channel and is
/// localised. Discord not running is a normal state, not an error.
#[cfg(windows)]
pub fn find() -> Option<Found> {
    use std::collections::HashMap;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    // pid -> (parent pid, variant index)
    let mut discord: HashMap<u32, (u32, usize)> = HashMap::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);

                if let Some(variant) = DISCORD_PROCESSES
                    .iter()
                    .position(|p| p.eq_ignore_ascii_case(&name))
                {
                    discord.insert(entry.th32ProcessID, (entry.th32ParentProcessID, variant));
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }

    // The root is the one whose parent is not itself a Discord process.
    // Prefer the earliest variant (stable over Canary over PTB).
    discord
        .iter()
        .filter(|(_, (parent, _))| !discord.contains_key(parent))
        .min_by_key(|(pid, (_, variant))| (*variant, **pid))
        .map(|(pid, (_, variant))| Found {
            pid: *pid,
            variant: *variant,
        })
}

#[cfg(target_os = "macos")]
pub fn find() -> Option<Found> {
    // TODO(phase-4): enumerate via sysctl / libproc.
    None
}

/// Human-readable name of a located variant.
pub fn variant_name(f: &Found) -> &'static str {
    DISCORD_PROCESSES
        .get(f.variant)
        .copied()
        .unwrap_or("Discord")
}
