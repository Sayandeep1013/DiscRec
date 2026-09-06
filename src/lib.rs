//! DiscRec — records Discord's audio. Press record.
//!
//! Architecture: process finder, capture backend, mixer, writer, session,
//! and a native window. Start at `docs/HANDOFF.md`.

pub mod capture;
pub mod cli;
pub mod discord;
pub mod mixer;
pub mod paths;
pub mod session;
pub mod writer;

#[cfg(windows)]
pub mod ui;

#[cfg(target_os = "macos")]
#[path = "macos_ui.rs"]
pub mod ui;
