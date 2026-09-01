# Handoff — start here

**Written 2026-09-02, end of session 1.** Everything below is verified unless
it says otherwise.

Two documents matter and they do different jobs:

- **This file** — where the project is, how to run it, what to do next.
- **[PROJECT-LOG.md](PROJECT-LOG.md)** — *why* things are the way they are. The
  scope changed three times and several confident conclusions were wrong. Read
  it before changing any decision, or you will re-litigate settled ground.

---

## 1. What exists right now

A working Windows recorder, driven from the command line. It captures Discord's
audio and your microphone, mixes them with clock-drift correction, and writes
Ogg/Opus.

**Verified working:**

| | Evidence |
|---|---|
| Finds Discord's root process | 6 processes present, correct root selected |
| Captures Discord audio only | Two-tone test: −21.1 dB target vs −41.6 dB excluded, symmetric under control |
| Captures a real Discord call | 45 s of live conversation, confirmed by ear |
| Captures the microphone | Both sides audible in one file, confirmed by ear |
| Drift correction holds | Buffer 4796–4814 against 4800 target over 12 min, zero underruns/clamps |
| Survives being killed | 25/25 kill cycles produced decodable files (R7) |
| Opus output | Decodes clean under `ffmpeg -f null`. 0.38 MB vs 16.5 MB WAV |
| Footprint | 24 MB resident, 0.68 MB release binary |

**Not built yet:** the app itself. There is no window, no record button, no
tray. `src/main.rs` is a development harness, not the product.

---

## 2. Environment — read before building

The toolchain is deliberately unusual because of a hard constraint: **nothing
installs to `C:`.** Everything lives on `D:`.

| | Location |
|---|---|
| Rust 1.98.0 `x86_64-pc-windows-gnu` | `D:\rust\rustup` |
| cargo / clippy / rustfmt | `D:\rust\cargo` |
| MinGW-w64 16.2.0 | `D:\mingw64` |
| CMake 4.4.3 | `D:\cmake` |
| ffmpeg 7.1.1 | `D:\ffmpeg\ffmpeg-7.1.1-full_build\bin` |

`CARGO_HOME` and `RUSTUP_HOME` are set at user scope; all four `bin`
directories are on the user PATH. A fresh shell should just work.

### If `cargo build` fails with `dlltool.exe: CreateProcess`

Run `scripts\fix-gnu-toolchain.ps1`.

rustup's bundled `dlltool` needs an assembler and resolves it *relative to its
own directory*, not via PATH — so having MinGW on PATH does not help. A
`rustup update` can remove the `as.exe` the script places. This is not rustup's
`dlltool` being broken; an earlier revision of ADR-0009 claimed that and was
wrong. → [adr/0009](adr/0009-gnu-toolchain-no-visual-studio.md)

### Why GNU and not MSVC

The Windows SDK cannot be kept off `C:`. The `windows` crate generates bindings
from Windows metadata rather than SDK headers, so no SDK is needed. C-dependent
crates are where this bites — Opus needed CMake, then a crate version bump.

---

## 3. Running it

```powershell
cargo build --release

# capture Discord only, 12 s, to capture.wav
cargo run --release -- 12

# both streams mixed to Ogg/Opus  <- the real path
cargo run --release -- 45 --mix

# add drift/buffer telemetry to soak.csv every 30 s
cargo run --release -- 720 --mix --log

# diagnostics
cargo run --release -- --devices          # list output endpoints
cargo run --release -- 10 --pid 1234      # capture an arbitrary process
cargo run --release -- 10 --system        # whole-system loopback
cargo run --release -- 20 --both          # per-stream stats, no mixing

.\scripts\crash-test.ps1 -Runs 25         # R7
cargo test                                # 15 unit tests
```

**Testing gotcha that cost real time:** any test needing a human to do something
must have a window long enough that coordination is not part of the
measurement. Twelve-second windows produced a confident, wrong conclusion that
Discord voice could not be captured at all. Use 45 s+.

---

## 4. Architecture in one screen

Four parts. Only one is platform-specific.

```
discord.rs      find Discord's ROOT pid (audio comes from a child;
                INCLUDE_TARGET_PROCESS_TREE covers it)
    |
capture/        <-- the ONLY platform-specific code
  windows.rs    WASAPI process loopback + default mic, two threads,
                two independent device clocks
  macos.rs      STUB. This is what a Mac contributor writes.
    |
mixer.rs        Discord is timeline master; mic is resampled to match.
                PI controller steered by buffer depth. Soft-knee limiter.
    |
writer.rs       Opus encode, Ogg pages straight to the OS (no BufWriter,
                so a kill cannot lose buffered audio)
```

A `#[cfg]` anywhere outside `src/capture/` is a design smell.

---

## 5. What to do next, in order

### 5.1 Finish Phase 2 — the 4-hour soak (R6)

The only remaining gate. Everything is in place; it needs uninterrupted hours
with Discord open, which was not available this session.

```powershell
cargo run --release -- 14400 --mix --log
```

Pass condition is **not** a small offset — it is **no monotonic trend** in
`smoothed_frames`, plus zero underruns and zero clamp hits.

**Analyse `smoothed_frames`, never `raw_frames`.** Raw depth jumps by a whole
packet depending on when it is sampled, producing two alternating branches; a
regression across a branch switch reported 19 ppm of drift that did not exist.

Best evidence so far: 12 minutes, buffer 4796–4814 against 4800, integral
bounded, no faults. Strong, but not four hours.

### 5.2 Measure CPU on a release build

**Outstanding and unverified.** The only CPU figure taken was 8.29% on a *debug*
build against a 3% budget (R11) — not a fair reading, and both release runs
exited before being sampled. Sample `TotalProcessorTime` while a release soak
is running. If it genuinely exceeds 3%, the likely cause is the 4 ms polling
loop in `pump`; the fix is event-driven capture via
`AUDCLNT_STREAMFLAGS_EVENTCALLBACK`.

### 5.3 Build the actual app (Phase 3)

Window, record button, level meters, tray indicator, first-run notice.
→ [spec/desktop-shell.md](spec/desktop-shell.md)

Exit criterion is a person recording a call within 10 seconds of first launch,
unassisted (R13). Keep it native — Electron fails the footprint requirements on
its own.

### 5.4 macOS (Phase 4)

Needs Mac hardware, which this machine does not have.
[spec/capture-macos.md](spec/capture-macos.md) is written from Apple's docs and
is **unverified**. Point a contributor at
[CONTRIBUTING-macos.md](CONTRIBUTING-macos.md); their first job is answering
five open questions, not writing code.

---

## 6. Traps

Ordered by how much time each cost.

1. **Short test windows.** Produced a confident, wrong "Discord voice cannot be
   captured" conclusion. It was measuring empty windows.
2. **Instruments that manufacture their own signal.** Three separate wrong
   conclusions this session came from the measurement rather than the system —
   the `dlltool` diagnosis, the voice-capture claim, and a drift regression
   contaminated by sampling phase. When a result is surprising, suspect the
   instrument first.
3. **A slow filter inside a fast loop.** Fixing jitter-chasing by adding
   smoothing created a worse oscillation. `CONTROLLER_GAIN` is now *derived*
   from a stated loop time constant; keep it that way and keep the test that
   asserts the loop is slower than its filter.
4. **Success codes that mean nothing.** Capture APIs return healthy streams
   carrying digital silence. Always measure signal.
5. **Struct layout for Win32.** `AUDIOCLIENT_ACTIVATION_PARAMS` has no padding;
   adding some made activation fail with a plausible-looking HRESULT.
6. **Python heredocs mangling Rust string escapes.** `\n` inside a `println!`
   became a literal newline more than once. Use the Write/Edit tools for Rust
   containing escapes.

---

## 7. Conventions

- Commits carry **no co-author or attribution trailers** — the user asked for
  this explicitly.
- Requirements are `R1..Rn` in [04-requirements.md](04-requirements.md);
  challenges `P1..Pn` in [05-challenges.md](05-challenges.md). Cite the ID.
- Decisions go in `adr/`. ADR-0001 is superseded; start from
  [ADR-0008](adr/0008-manual-control.md).
- Before adding a feature, check it against
  [09-alternatives.md](09-alternatives.md). If OBS already does it better, it
  does not belong here — the entire differentiator is one button, one file,
  small.

## 8. Open questions

| # | Question | Blocks |
|---|---|---|
| 1 | Does drift hold over 4 hours with no monotonic trend? | R6, Phase 2 |
| 2 | Real CPU on a release build? | R11 |
| 3 | Do macOS tapped streams arrive attenuated? | Phase 4 design |
| 4 | Does Discord keep an audio session when idle in a channel, or only when someone speaks? | Affects UX when nobody is talking |
