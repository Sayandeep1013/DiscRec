# Challenges and fixes

Ordered by cost when found late. P1 and P2 are the ones that produce files which
exist, play, and are wrong — they get the most attention because nothing else in
the system will catch them.

---

## P1 — Clock drift between the two capture streams {#p1}

**Severity: critical. Silent, and permanent once written.**

Discord's loopback endpoint and the microphone are separate devices with
separate hardware oscillators ([C3](02-constraints.md)). Nominally both run at
48 kHz. Neither actually does, and the difference accumulates.

**Measured, not assumed.** On the development machine the loopback ran at
47995.7 Hz (−89 ppm) and the microphone at 48007.2 Hz (+151 ppm): a relative
drift of **−240 ppm, about 860 ms per hour.**

This document originally estimated ~20 ppm and ~72 ms per hour. The real figure
is roughly an order of magnitude worse, and nearly a second per hour is grossly
audible — which makes drift compensation load-bearing rather than a refinement.
→ [spec/mixing-and-timeline.md](spec/mixing-and-timeline.md) for how the
measurement was taken, and the three ways it was wrong first.

Because the mix is written at capture time, **the error cannot be corrected
afterwards.** By the time anyone notices, every recording made so far has your
voice sliding against everyone else's.

**Fix.** Do not assume the two streams advance together.

- Track each stream's own sample position independently. Never use arrival
  time — the `Frame` type carries `sample_pos` and no timestamp field, so the
  wrong clock is not reachable.
- Designate the Discord stream as the timeline master; resample the microphone
  to it continuously, correcting fractional-rate error rather than dropping or
  duplicating whole samples.
- On macOS, an aggregate device with drift compensation enabled does much of
  this in the OS. On Windows the two WASAPI clients are genuinely independent
  and it must be done explicitly.
- Log both stream positions once a minute. A widening gap is the early warning.

**Verify:** the 4-hour soak (R6). Pass requires alignment within 50ms *and* no
monotonic trend — a steady drift under threshold still means the mechanism is
wrong.

---

## P2 — Capture succeeds and records silence {#p2}

**Severity: high. Looks exactly like success.**

Both platforms can hand back a working stream that carries nothing: wrong PID,
process tree not included, or a permission that was granted in the dialog but
is not in effect ([C2](02-constraints.md)).

**Fix.** Assert signal, not status codes. Measure RMS over the first ~3 seconds
of each stream; if it is pure digital silence, raise `capture_silent` rather
than continuing. Distinguish "Discord is running but nobody is talking yet" by
checking the loopback stream is *active*, not merely open — and prefer a false
alarm the user can dismiss over a silent recording they discover later.

---

## P3 — macOS permissions and notarization {#p3}

Process taps sit behind a TCC prompt, and Gatekeeper rejects unsigned binaries
requesting audio capture ([C4](02-constraints.md)).

**Fix.** Treat as a work item, not a build step: Apple Developer account,
hardened runtime, signing, notarization, stapling. Trigger the permission prompt
at first launch and verify real signal then, so a denial surfaces before someone
relies on a recording. Handle denial as a first-class UI state with a direct
link to the settings pane.

This is the largest scheduling risk in the project and needs hardware the
primary developer does not have
([CONTRIBUTING-macos.md](CONTRIBUTING-macos.md)).

---

## P4 — Mixing is irreversible {#p4}

One mixed track was chosen deliberately, and it means a badly-set microphone
level ruins a recording with no recovery — the voices cannot be pulled apart
again.

**Fix.** Show live level meters for both sources *before* recording starts, so a
wrong level is visible rather than discovered. Apply a limiter to the sum (R9);
clipping two loud sources together is otherwise routine. Consider conservative
default headroom on the microphone.

---

## P5 — "Lightweight" is decided by the UI framework {#p5}

Electron is ~150 MB on disk and ~200 MB resident before recording anything,
failing R10 and R12 by itself. The capture and mixing code is trivial by
comparison, and the framework decision is the expensive one to reverse.

**Fix.** Native Rust with a minimal window and tray
([ADR-0002](adr/0002-language-and-runtime.md)). Make the choice once, at the
start. If the app is not dramatically smaller than OBS it has no reason to exist
([09-alternatives.md](09-alternatives.md)).

---

## P6 — Per-process capture may not be portable in one library {#p6}

The obvious approach is one cross-platform crate for both backends. Verified in
Sept 2026: `cpal` has loopback on both platforms but **system-wide only**, which
fails R2. `flexaudio` claims per-process on both but is very young.

**Fix.** Spike before committing ([ADR-0007](adr/0007-cross-platform-strategy.md)).
The capture trait means the choice is contained to one module either way. If no
library holds up, write the two backends natively — the specs describe the
mechanism regardless.

**Fallback position:** R2 is `MUST` today. If per-process capture turns out to
be what blocks macOS, dropping to system-wide capture is a smaller loss than
shipping no Mac build — but that is a decision to take deliberately, not by
drift.
