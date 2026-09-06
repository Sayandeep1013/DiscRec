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
    let procs = macos_list();
    if procs.is_empty() {
        return None;
    }
    let pids: std::collections::HashSet<u32> = procs.iter().map(|p| p.pid).collect();
    procs
        .iter()
        .filter(|p| p.variant.is_some() && !pids.contains(&p.ppid))
        .min_by_key(|p| (p.variant.unwrap(), p.pid))
        .map(|p| Found {
            pid: p.pid,
            variant: p.variant.unwrap(),
        })
}

/// Root Discord PID plus every helper in its tree. The tap must name all of
/// them — audio is rendered by a child, same as Windows process-tree loopback.
#[cfg(target_os = "macos")]
pub fn descendant_pids(root: u32) -> Vec<u32> {
    let procs = macos_list();
    let mut kids: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for p in &procs {
        kids.entry(p.ppid).or_default().push(p.pid);
    }
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        if let Some(ch) = kids.get(&out[i]) {
            for &c in ch {
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        i += 1;
    }
    out
}

#[cfg(target_os = "macos")]
struct MacProc {
    pid: u32,
    ppid: u32,
    variant: Option<usize>,
}

#[cfg(target_os = "macos")]
fn macos_list() -> Vec<MacProc> {
    unsafe { macos_list_inner() }
}

#[cfg(target_os = "macos")]
unsafe fn macos_list_inner() -> Vec<MacProc> {
    let bytes = libc::proc_listpids(libc::PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0);
    if bytes <= 0 {
        return Vec::new();
    }
    let cap = bytes as usize / std::mem::size_of::<i32>();
    let mut pids = vec![0i32; cap];
    let wrote = libc::proc_listpids(
        libc::PROC_ALL_PIDS,
        0,
        pids.as_mut_ptr().cast(),
        (pids.len() * std::mem::size_of::<i32>()) as i32,
    );
    if wrote <= 0 {
        return Vec::new();
    }
    pids.truncate(wrote as usize / std::mem::size_of::<i32>());

    let mut out = Vec::new();
    for pid in pids.into_iter().filter(|&p| p > 0) {
        let mut name = [0u8; 128];
        let n = libc::proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32);
        if n <= 0 {
            continue;
        }
        let name = String::from_utf8_lossy(&name[..n as usize]).into_owned();
        if !is_discord_family(&name) {
            continue;
        }
        let variant = DISCORD_PROCESSES
            .iter()
            .position(|p| name.eq_ignore_ascii_case(p));
        out.push(MacProc {
            pid: pid as u32,
            ppid: macos_ppid(pid as u32).unwrap_or(0),
            variant,
        });
    }
    out
}

#[cfg(target_os = "macos")]
fn is_discord_family(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "discord"
        || n.starts_with("discord ")
        || n.starts_with("discord helper")
        || n.starts_with("discord canary")
        || n.starts_with("discord ptb")
}

#[cfg(target_os = "macos")]
fn macos_ppid(pid: u32) -> Option<u32> {
    #[repr(C)]
    struct ProcBsdShortInfo {
        pbsi_pid: u32,
        pbsi_ppid: u32,
        pbsi_pgid: u32,
        pbsi_status: u32,
        pbsi_comm: [u8; 16],
        pbsi_flags: u32,
        pbsi_uid: u32,
        pbsi_gid: u32,
        pbsi_ruid: u32,
        pbsi_rgid: u32,
        pbsi_svuid: u32,
        pbsi_svgid: u32,
        pbsi_rfu: u32,
    }
    // PROC_PIDT_SHORTBSDINFO = 13 on Darwin.
    const PROC_PIDT_SHORTBSDINFO: i32 = 13;
    let mut info = unsafe { std::mem::zeroed::<ProcBsdShortInfo>() };
    let got = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            PROC_PIDT_SHORTBSDINFO,
            0,
            (&mut info as *mut ProcBsdShortInfo).cast(),
            std::mem::size_of::<ProcBsdShortInfo>() as i32,
        )
    };
    if got as usize == std::mem::size_of::<ProcBsdShortInfo>() {
        Some(info.pbsi_ppid)
    } else {
        None
    }
}

/// Human-readable name of a located variant.
pub fn variant_name(f: &Found) -> &'static str {
    DISCORD_PROCESSES
        .get(f.variant)
        .copied()
        .unwrap_or("Discord")
}
