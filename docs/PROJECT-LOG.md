# Project log

**If you are starting a fresh session, read this file first.** It carries the
context that is not recoverable from the code or the commit history: why the
scope changed three times, what was researched and rejected, and which
conclusions were reversed.

Last updated: 2026-09-01, end of Phase 1.

---

## 1. What DiscRec is, right now

A small Windows/macOS app that records Discord calls. **You open it and press
record.** It finds Discord's process, captures Discord's audio and your
microphone, mixes them into one Ogg/Opus file, and stops when you say so.

No background service. No bot. No auto-start. No configuration to speak of.

**Positioning:** OBS already does this, on both platforms, using the same OS
APIs. DiscRec is not a new capability — it is that one job as one binary with
one button. The differentiator is **setup cost and footprint**, nothing else.
If someone already has OBS configured, this offers them nothing, and
`09-alternatives.md` says so plainly.

---

## 2. How the scope got here

The project changed direction three times. Each change was correct, and the
reasoning matters more than the outcome.

### v1 — "record Discord calls on every OS, auto-start on join"

The original ask: Windows, Mac, Android; starts by itself when you join a voice
channel; records everyone; very lightweight.

Research immediately found two hard walls:

- **Android cannot record Discord voice.** `AudioPlaybackCapture` admits only
  `USAGE_MEDIA`, `USAGE_GAME`, `USAGE_UNKNOWN`. Discord voice is
  `USAGE_VOICE_COMMUNICATION` — the same protection that stops apps recording
  phone calls. No permission lifts it. iOS has no capture API at all.
- **Discord end-to-end encrypts all voice** (DAVE, enforced 2 March 2026;
  clients without it get close code `4017`). Any design based on intercepting
  network traffic is permanently dead.

### v2 — bot-primary

That left two viable capture points: a bot inside the call's encryption group
(per-person tracks), or local OS capture after decryption (mixed audio).

Chose the bot (ADR-0001), because it gave per-person tracks, worked from any
device, and could honor an opt-out.

Then mobile was dropped entirely (ADR-0006), voiding one of those reasons.

### v3 — current: local capture, manual control

The user reframed the goal: *"what we're building is just OBS, only specifically
records Discord's audio. Super lightweight. Windows and Mac. OBS is heavy and
hard to set up — I want a one-place solution."*

Then simplified further: *"let's keep everything controlled via user — user
opens the app and presses record."*

That second change deleted more than anything else in the project: the entire
join-detection subsystem, the Discord RPC dependency and its 50-user approval
cap, the heuristic detector, the background daemon, the local control API, and
the consent-enforcement machinery. **Nine components became four.**

ADR-0008 records it and supersedes ADR-0001.

---

## 3. Decisions

| ADR | Decision | Status |
|---|---|---|
| [0001](adr/0001-primary-capture-route.md) | Bot route primary | **Superseded by 0008** |
| [0002](adr/0002-language-and-runtime.md) | Rust, no Electron | Accepted |
| [0003](adr/0003-capture-abstraction.md) | One capture trait, two backends | Accepted |
| [0004](adr/0004-storage-opus-passthrough.md) | Store Opus, never WAV | Accepted, amended |
| [0005](adr/0005-timeline-dual-clock.md) | Position from stream clock, not wall clock | Accepted |
| [0006](adr/0006-mobile-out-of-scope.md) | No mobile, ever | Accepted |
| [0007](adr/0007-cross-platform-strategy.md) | Capture-layer strategy | **Resolved in Phase 1** — native `windows` crate |
| [0008](adr/0008-manual-control.md) | Manual control, no auto-start | Accepted |
| [0009](adr/0009-gnu-toolchain-no-visual-studio.md) | GNU toolchain on Windows | Accepted |

---

## 4. Research that cost real effort — do not redo

### DAVE / Discord voice encryption

Discord completed E2EE rollout for all voice; since 2 March 2026 a client
without DAVE cannot connect. Media on the wire is ciphertext Discord itself
cannot read.

**This is irrelevant to the current product.** DiscRec captures audio after
Discord decrypts and renders it. It is recorded because it permanently rules out
network interception, which is the first thing anyone proposes.

Full findings: `research/dave-protocol.md`, `research/prior-art-craig.md`.

### The reversal worth knowing about

Mid-project I concluded that bot-based recording was blocked because no library
implemented DAVE *receive* — py-cord 2.8.0 shipped sending only, and
`@discordjs/voice` 0.19.x had receive broken with open unfixed issues.

**That was wrong**, and the correction changed the architecture at the time. It
was true of the mainstream libraries and false of the ecosystem. Querying
Craig's repository directly showed it still recording, with commits landing that
day, and its dependency list gave the answer:

- **`@snazzah/davey`** — MIT, OpenMLS-based DAVE implementation, published for
  **Rust, Node and Python**. The only working non-Discord implementation found.
- **`@projectdysnomia/dysnomia`** — Eris fork with a DAVE-aware voice
  connection. Notably *not* discord.js.

Lesson: web search summaries were wrong about this twice. Reading the actual
repository and `npm view` settled it in two commands.

This material is preserved in `deferred/` because it is the only route to
per-person tracks if that ever becomes worth the setup burden.

### Platform capture APIs

Both target OSes gained per-process audio capture recently; before ~2021 this
needed a virtual audio driver or kernel extension.

| OS | API | Minimum |
|---|---|---|
| Windows | WASAPI process loopback | Win10 build 20348 |
| macOS | Core Audio process taps | macOS 14.2 (14.4+ preferred) |

**`cpal` cannot be used as-is.** Verified by reading v0.18.2's source: its macOS
loopback calls `AudioHardwareCreateProcessTap` with an *empty* process list and
`setExclusive(true)` — "exclude nothing", i.e. record everything. Windows uses
device-level `AUDCLNT_STREAMFLAGS_LOOPBACK` with no process-loopback usage
anywhere in the crate. Both are **system-wide**, which fails R2.

**`flexaudio` 0.2.0** claims per-process on Windows, macOS and Linux via a
`target_pid`, MIT, Rust core. Exactly the right API surface — but one crates.io
release, ~448 downloads, 5 stars, one maintainer. Unproven.

That trade-off was ADR-0007. **The Phase 1 spike resolved it: native.** See
"ADR-0007 resolved" below.

---

## 5. Rejected approaches — do not re-propose

| Approach | Why not |
|---|---|
| Intercept network traffic | E2EE. Discord's own servers cannot read it |
| Android or iOS recording | Platform-blocked, not a permissions problem |
| Android as a remote control | Dropped — not worth its toolchain for a control-only app |
| Discord client mods / self-bots | ToS violation, account termination, and produce nothing per-process capture doesn't |
| Reverse-engineered DAVE decryption | Deliberately outside the protocol's threat model; worse posture than recording a call you are in |
| Electron | ~150 MB disk, ~200 MB resident. Fails the footprint requirements by itself |
| Auto-start / join detection | ADR-0008. Removed the most fragile subsystem in the design |
| Per-person tracks | Needs a hosted bot + Discord app + server admin. Contradicts the zero-setup premise |
| MSVC toolchain | Windows SDK cannot be kept off the C: drive |

---

## 6. The two defects that drive the design

Both are **silent** — they produce files that exist, play, and are wrong.
Nothing in the system catches them except deliberate tests.

### P1 — clock drift between the two capture streams

Discord's loopback and the microphone are separate devices with separate
hardware oscillators. Both claim 48 kHz; neither is exactly 48 kHz. At ~20 ppm
(ordinary consumer hardware) they separate by ~72 ms per hour.

Because the mix is written at capture time, **this cannot be fixed afterwards.**

Mitigation: position every frame by its own device's sample counter, treat
Discord as timeline master, and continuously correct the microphone's resample
ratio. macOS may get much of this free via aggregate-device drift compensation;
Windows must do it explicitly. `spec/mixing-and-timeline.md`.

Gated by a 4-hour soak test that must show no monotonic trend, not merely a
small offset.

### P2 — capture succeeds and records silence

Both platforms can return a healthy stream carrying nothing: wrong PID, process
tree not included, or a permission granted in the dialog but not in effect.

Mitigation: assert RMS over the first ~3 seconds, return `NoSignal` rather than
writing a file.

---

## 7. Current state

### Done

- Full specification: 40 docs, all internal links verified resolving
- **Phase 0** — toolchain installed on D:, verified
- **Phase 1 — per-process capture works and isolation is proven**

#### Phase 1 results

`src/discord.rs` finds Discord's **root** process. Discord runs a tree of ~6
identically-named processes; the finder returns the one whose parent is not
also Discord, which is the PID that `INCLUDE_TARGET_PROCESS_TREE` needs.
Verified against a live Discord: 6 processes present, correct root selected.

`src/capture/windows.rs::record_to_wav` captures a process's audio to WAV via
WASAPI process loopback.

**Isolation is proven, not assumed.** Two `ffplay` instances were run
simultaneously, one at 440 Hz and one at 1000 Hz, and each was captured in turn
while the other played:

| Captured | 440 Hz band | 1000 Hz band |
|---|---|---|
| the 440 Hz process | **-21.1 dB** | -41.6 dB |
| the 1000 Hz process | -40.6 dB | **-21.1 dB** |

Symmetric. The captured tone reads -21.1 dB both times; the other sits ~20 dB
down at the bandpass filter's own leakage floor. The non-target process is not
in the recording. **R2 satisfied.**

Timing is exact: 286,560 frames for a 6-second capture at 48 kHz.

#### Discord voice capture confirmed — and a false alarm on the way

Captured a live Discord call, 45 s, via process loopback on the root PID:
peak 0.5581, mean -28.4 dB, and ~15.5 s of signal spread across six segments
separated by 3.7-5.8 s gaps. That alternating pattern is two people talking with
natural pauses. **Phase 1's exit criterion is met.**

Getting there produced a wrong conclusion worth recording, because the failure
mode will recur.

Short test windows said voice was *not* captured. A 12 s run during continuous
speech returned peak 0.0000 and 11.98 s of digital silence. Mute/unmute beeps
captured fine at 0.7241. The obvious reading was that Windows excludes
communications-category streams from process loopback -- the same protection
that blocks Android -- which would have been product-ending.

**That reading was wrong.** The captures started the instant the command ran,
giving no time to read the instruction and get another person talking before the
window closed. The tests were measuring empty windows, not a platform
restriction. Lengthening the window to 45 s with no coordination required showed
the voice plainly.

Lesson: when a test needs a human to do something, the window has to be long
enough that coordination is not part of the measurement. Two confident
conclusions this session came from tooling artifacts rather than the system
under test -- see also the `dlltool` diagnosis in ADR-0009.

#### Endpoint enumeration added

Investigating the false alarm revealed the machine has two active render
endpoints (Speaker and Headphone on one Realtek codec), with Headphone as the
Windows default. `--devices` lists them and `--device N` captures one, because
"the default endpoint" is not necessarily where a given application's audio
goes. Useful diagnostic, and the basis of any future output-device selection.

#### Two bugs found and fixed during Phase 1

1. **`AUDIOCLIENT_ACTIVATION_PARAMS` padding.** The struct is a 4-byte enum
   followed *directly* by an 8-byte union — 12 bytes, no padding. An added
   `_pad: u32` made the driver read the process id from the wrong offset and
   activation failed. This is the kind of error that produces a plausible-looking
   HRESULT rather than an obvious crash.
2. **Wrong format tag.** The capture buffer is read as `f32`, so the stream must
   be declared `WAVE_FORMAT_IEEE_FLOAT` (3), not `WAVE_FORMAT_PCM`.

Also: an early error path mapped *any* failing activation HRESULT to
"unsupported OS", which was actively misleading on a Windows 11 build 26200
machine. It now reports the real HRESULT and only special-cases `E_NOTIMPL`.

#### The P2 failure mode, observed for real

The first successful activation captured 383,040 frames of pure silence. That is
exactly the [P2](05-challenges.md#p2) shape — a healthy stream carrying nothing —
and it was indistinguishable from a bug until tested against a known-noisy
process. The `--pid` test hook exists for that reason, and the lesson is in the
spec: **never trust a success code, measure the signal.**

In this case Discord genuinely was not making sound.

### Environment

Everything on `D:`; nothing installed to `C:` (a hard constraint from the user).

| | |
|---|---|
| Rust 1.98.0 `x86_64-pc-windows-gnu` | `D:\rust\rustup` |
| cargo, clippy, rustfmt | `D:\rust\cargo` |
| `CARGO_HOME` / `RUSTUP_HOME` | set at user scope to the above |
| Build artifacts | project-local `target/` |
| ffmpeg 7.1.1 | `D:\ffmpeg\ffmpeg-7.1.1-full_build\bin`, on PATH |
| Repo | `https://github.com/Sayandeep1013/DiscRec` |

Verified clean: `cargo build`, `cargo clippy -- -D warnings`,
`cargo fmt --check`, `cargo run`.

**Commit convention: no co-author or attribution trailers.** The user asked for
this explicitly.

### ADR-0007 resolved

The spike answered it: **native, via the `windows` crate.** `flexaudio` has 448
downloads, one release and one maintainer — too thin to be load-bearing on the
capture path, and almost certainly untested on the GNU toolchain. `windows`
0.62.2 has 310M downloads and explicit GNU support. macOS will be written
natively too, so a shared crate would only have paid off if it worked well on
both.

### Next — Phase 2

Add the microphone as a second stream, drift compensation between the two
clocks, the limiter, Opus encoding, and incremental crash-safe writes. Refactor
`record_to_wav`'s inner loop to drive the `CaptureBackend` trait instead of a
WAV writer.

Exit criterion is the 4-hour soak: alignment within 50 ms **with no monotonic
trend**, plus `SIGKILL` at 100 random offsets leaving playable files.

---

## 8. Open questions and risks

| # | Question | Impact |
|---|---|---|
| 1 | **Does per-process capture work via a crate, or must it be native?** | Decides whether macOS is a port or just a build target. ADR-0007, first Phase 1 task |
| 2 | **macOS is unbuildable here.** No Mac, no Apple Developer account | Half the product. `spec/capture-macos.md` is written from documentation and is **unverified**. Mitigation: a contributor clones and implements one file — `CONTRIBUTING-macos.md` |
| 3 | **Do macOS tapped streams arrive attenuated?** Open Apple forum thread | If levels are wrong, everything downstream is wrong. Answerable only on hardware |
| 4 | **Does a GNU-target dependency break?** | ADR-0009. Likely candidate is an audio crate with a C component. Fallback is `xwin` |
| 5 | Does drift compensation actually hold over 4 hours? | Gates Phase 2. Unknown until measured |

---

## 9. Working agreements

- Targets are **Windows and macOS**. Linux is a sketch in `deferred/`, not a
  commitment.
- Platform-specific code lives **only** in `src/capture/{windows,macos}.rs`. A
  `#[cfg]` anywhere else is a design smell.
- Every feature request gets checked against `09-alternatives.md`. If the answer
  is "OBS already does that, better", it does not belong here — adding it moves
  DiscRec toward being a worse OBS.
- Library facts age fast. Re-verify against `npm view`, `crates.io`, or the
  actual repository rather than trusting search summaries or this document.
