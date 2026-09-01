# Roadmap

Every phase has an exit criterion that is a test, not a feeling.

---

## Phase 0 — Toolchain · ~1 hour

Install Rust and the MSVC build tools
([08-toolchain-and-gaps.md](08-toolchain-and-gaps.md)). Create the workspace
skeleton.

**Exit:** `cargo build` succeeds on Windows.

---

## Phase 1 — Prove the capture · ~2 days

The spike from [ADR-0007](adr/0007-cross-platform-strategy.md): capture
Discord's process audio on Windows and write raw PCM to disk. No mic, no mixing,
no encoding, no UI.

Decide here whether a cross-platform crate covers both backends, or whether both
are written natively. Verify **per-process** capture specifically — a library
that only does system-wide capture fails R2 ([P6](05-challenges.md#p6)).

**Why first:** everything else assumes clean isolated Discord audio exists. If
it does not, nothing downstream matters.

**Exit:** a WAV of a Discord call containing the call and not the music playing
alongside it.

---

## Phase 2 — Make it a recorder · ~1 week

Add the microphone, drift compensation, the limiter, Opus encoding, and
incremental crash-safe writes. Still command-line only.

**Why second:** [P1](05-challenges.md#p1) invalidates every recording made
before it is fixed, and the error cannot be repaired afterwards. Fixing drift
after accumulating recordings means those recordings are already wrong.

**Exit — this gates the project:**

- 4-hour soak, sync tone every 15 minutes, alignment within 50ms with **no
  monotonic trend** (R6)
- `SIGKILL` at 100 random offsets, every resulting file plays (R7)
- Wrong-PID and denied-permission cases raise errors rather than writing silence
  (R8)

---

## Phase 3 — The app · ~4 days

Window with record/stop, elapsed time, live level meters, tray indicator, and
show-in-folder. First-run reminder about telling people
([06-legal-and-consent.md](06-legal-and-consent.md)).

**Exit:** a person who has never seen it records a call within 10 seconds of
first launch, unassisted (R13). Footprint measured under 40 MB and 3% CPU
(R10, R11).

---

## Phase 4 — macOS · ~2 weeks, highest uncertainty

Core Audio process taps behind the same trait, then signing and notarization.
Expect notarization to be most of it ([P3](05-challenges.md#p3)).

**Cannot be started from the primary development machine** — it needs Mac
hardware and an Apple Developer account, and
[spec/capture-macos.md](spec/capture-macos.md) is written from documentation
rather than experience.

→ [CONTRIBUTING-macos.md](CONTRIBUTING-macos.md) is the onboarding for whoever
does this.

**Exit:** clean-VM run on macOS produces a valid recording with correct levels,
and a denied permission produces a clear blocked state rather than a silent file.

---

## Deliberately deferred

Export to other formats; retention policy; device selection UI; anything in
[deferred/](deferred/README.md).
