//! DiscRec — records Discord's audio. Press record.
//!
//! No arguments opens the window. Flags still run the development harness
//! (`discrec --help`).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    // Must run before any HWND, or Windows bitmap-scales the whole process
    // (blurry window, blurry folder picker, blurry Explorer launched from us).
    #[cfg(windows)]
    discrec::ui::enable_dpi_awareness();

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        #[cfg(windows)]
        discrec::ui::attach_console();
        discrec::cli::run(args);
        return;
    }

    #[cfg(windows)]
    if let Err(e) = discrec::ui::run() {
        discrec::ui::attach_console();
        eprintln!("DiscRec failed to open: {e}");
        std::process::exit(1);
    }

    #[cfg(target_os = "macos")]
    if let Err(e) = discrec::ui::run() {
        eprintln!("DiscRec failed to open: {e}");
        std::process::exit(1);
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        eprintln!(
            "DiscRec targets Windows and macOS. See docs/CONTRIBUTING-macos.md."
        );
        std::process::exit(1);
    }
}
