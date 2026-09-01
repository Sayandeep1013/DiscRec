//! DiscRec — records Discord's audio. Press record.
//!
//! Start with `docs/README.md`. The architecture is four parts:
//! process finder, capture backend, mixer, writer.

mod capture;
mod discord;

fn main() {
    println!("DiscRec {}", env!("CARGO_PKG_VERSION"));

    match discord::find_pid() {
        Some(pid) => println!("Discord found: pid {pid}"),
        None => println!("Discord not running (process lookup not implemented yet)"),
    }

    let backend = capture::backend();
    let fmt = backend.format();
    println!(
        "Capture backend ready: {} Hz, {} ch",
        fmt.sample_rate, fmt.channels
    );

    println!("Nothing records yet — see docs/07-roadmap.md, Phase 1.");
}
