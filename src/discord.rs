//! Finding Discord's process to attach to.
//!
//! Not "detect that a call started" — auto-start was removed
//! (`docs/adr/0008-manual-control.md`). This is a process lookup.

// Scaffolding until Phase 1 implements process enumeration.
#![allow(dead_code)]

/// Executable names to look for, in preference order.
#[cfg(windows)]
pub const DISCORD_PROCESSES: &[&str] = &["Discord.exe", "DiscordCanary.exe", "DiscordPTB.exe"];

#[cfg(target_os = "macos")]
pub const DISCORD_PROCESSES: &[&str] = &["Discord", "Discord Canary", "Discord PTB"];

/// Locate Discord, preferring the instance with an active audio session.
///
/// Never match on window title — it changes with the active channel and is
/// localised. Discord not running is a normal state, not an error.
pub fn find_pid() -> Option<u32> {
    // TODO(phase-1): enumerate processes by name, prefer one with an
    // active audio session.
    None
}
