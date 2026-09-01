# Spec — Mixing and timeline

Satisfies R3, R6, R9. Addresses [P1](../05-challenges.md#p1),
[P4](../05-challenges.md#p4).

**The hardest part of this project.** The defect it prevents does not throw,
does not log, and does not show up in a five-minute test. It shows up when
someone plays back an hour-long recording and their own voice answers questions
half a second before they were asked.

## The problem

Two capture streams, two hardware clocks ([C3](../02-constraints.md)). Both
claim 48 kHz. Neither is exactly 48 kHz, and they are not the same amount wrong.

At 20 parts per million — ordinary for consumer hardware — the streams separate
by about 72 ms per hour. That is well past audible for voice.

Because the mix is written at capture time, **there is no post-processing fix.**
The two sources are summed into one waveform and cannot be pulled apart.

## The rule

> Every frame's position comes from its own device's sample counter.
> Never from when the callback fired.

`Frame` carries `sample_pos` and no timestamp, so the wrong clock is not
reachable from the mixing path ([capture-interface.md](capture-interface.md)).

## Approach

**Discord is the timeline master.** Its stream defines output position; the
microphone is continuously corrected to match.

```
discord frames ──▶ ring buffer A ──┐
                                   ├──▶ sum ──▶ limiter ──▶ writer
mic frames ──▶ resampler ──▶ ring B ┘
                   ▲
                   └── rate correction from measured drift
```

Each tick:

1. Read both streams' `sample_pos`.
2. `drift = mic_pos - expected_mic_pos_given_discord_pos`
3. Feed that error into a slow controller adjusting the resampler's ratio —
   nudging it by parts per million, not by dropping or inserting samples.
4. Sum the aligned buffers.

**Correct the rate, never the sample count.** Dropping or duplicating whole
samples to fix alignment is audible as clicks and, worse, converts a smooth
error into a jumpy one that is harder to detect.

The controller must be slow — seconds, not milliseconds — or it will chase
jitter and introduce pitch wobble.

## Platform differences

**macOS:** an aggregate device containing both the tap and the input can enable
drift compensation (`kAudioSubTapDriftCompensationKey`), letting Core Audio
handle much of this. Still measure and log the residual; do not assume it is
zero.

**Windows:** the two WASAPI clients are genuinely independent. There is no OS
drift compensation. It must be done explicitly, and this is where the work is.

## Gaps and glitches

If a stream stalls — device change, buffer overrun — its `sample_pos` jumps.

- Pad the missing samples with silence, exactly as many as the position
  difference indicates. Never close the gap by shifting audio, which would
  desynchronise everything after it.
- Count and log every gap. A rising count means something upstream is wrong.

## Summing and the limiter

Two sources summed will clip. Voices are not quiet, and Discord's output is
already normalised.

- Sum in f32, where intermediate overshoot is harmless.
- Apply a limiter before conversion: soft knee, fast attack, slow release.
- Leave headroom rather than normalising to full scale.

Because mixing is irreversible ([P4](../05-challenges.md#p4)), the shell shows
live meters for both sources **before** recording starts, so a badly-set
microphone is visible rather than discovered afterwards.

## Verification — gates Phase 2

R6 is not satisfied by a short manual check.

1. Four hours, sync tone into both the microphone and the Discord side every 15
   minutes.
2. Include a device change and a forced stall.
3. Cross-correlate the tone pairs at every sync point.

**Pass: offset under 50 ms at every point, and no monotonic trend.**

The second condition matters as much as the first. Steady growth that stays
under threshold still means the mechanism is wrong — it just has not run long
enough yet.

Run this before accumulating recordings worth keeping.
