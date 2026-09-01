# Setting up the macOS side

You are reading this because you have a Mac and the rest of the project does
not. **Everything except one file already works.** Your job is
`src/capture/macos.rs` — one file, behind a trait that already exists.

No fork, no separate branch, no parallel build. Clone, build, implement, push.

---

## What you need

| Requirement | Notes |
|---|---|
| **macOS 14.2 minimum, 14.4+ strongly preferred** | Core Audio process taps do not exist below 14.2. Check: `sw_vers -productVersion` |
| **Rust** | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| **Xcode Command Line Tools** | `xcode-select --install`. The full Xcode app is not required |
| **Discord** | Any client. Have a friend on a call, or a second account, to make real audio |
| Apple Developer account (~$99/yr) | **Only for distribution.** Not needed to build or test locally. Ignore until it matters |

## Getting started

```bash
git clone https://github.com/Sayandeep1013/DiscRec.git
cd DiscRec
cargo build
```

It will compile. `src/capture/macos.rs` is a stub returning
`CaptureError::Platform("not implemented")`, so the app runs and the window
opens — it just cannot record yet. Everything around it is done.

## Read these three, in order

1. **[spec/capture-interface.md](spec/capture-interface.md)** — the trait you
   implement and the five rules a backend must honor. Start here.
2. **[spec/capture-macos.md](spec/capture-macos.md)** — the mechanism, written
   from Apple's docs. **It is unverified.** Nobody has run it. Where reality
   disagrees with it, reality wins and the spec gets corrected.
3. **[spec/mixing-and-timeline.md](spec/mixing-and-timeline.md)** — why
   `sample_pos` must come from the device's own counter. This is the one thing
   that is genuinely easy to get wrong and impossible to fix later.

## Before writing much code: answer five questions

The spec has open unknowns that only hardware can settle. Answering them is
worth more than a partial implementation, because two of them could change the
design.

1. **Do tapped streams arrive attenuated?** There is an open Apple developer
   thread about per-device attenuation and getting unattenuated app audio. If
   levels come through wrong, everything downstream is wrong. **Highest
   priority.**
2. Does a tap survive Discord restarting, or must it be rebuilt?
3. What happens if the tap is created while Discord is not running?
4. Can an aggregate device hold both the tap and the microphone with drift
   compensation (`kAudioSubTapDriftCompensationKey`) actually working? If yes,
   macOS gets for free the hardest problem Windows has to solve by hand.
5. How does sample-rate negotiation behave if the aggregate disagrees with
   48 kHz?

Write the answers into `spec/capture-macos.md` and open a PR with just that.
That alone is a genuinely useful contribution.

## The reference implementation

[AudioCap](https://github.com/insidegui/AudioCap) is the community reference for
process taps, and exists because Apple's own documentation is thin. Read it
before fighting the API. It is Swift; the Core Audio calls translate directly.

## The one rule that matters most

**Never report success for a stream that carries no audio.**

Both platforms can hand back a healthy-looking stream that delivers digital
silence — wrong process, or a permission granted in the dialog but not in
effect. Measure RMS over the first ~3 seconds and return `CaptureError::NoSignal`
if it is pure silence.

The alternative is someone discovering next week that an hour-long recording is
empty. → [05-challenges.md](05-challenges.md#p2)

## Permissions while developing

macOS will prompt for audio capture the first time you run it. If you dismiss
it, you get silence rather than an error — which is exactly the failure above.

```bash
tccutil reset Microphone            # re-trigger the prompts while testing
tccutil reset AudioCapture
```

Add `NSAudioCaptureUsageDescription` and a microphone usage string to
`Info.plist`, or the prompt never appears at all.

## Testing your work

```bash
cargo test                       # timeline and mixer tests, platform-neutral
cargo run                        # the actual app
```

Then, in order:

1. Record 30 seconds of a Discord call. Confirm you can hear both sides.
2. Play music while recording. **It must not be in the file** — that is
   requirement R2 and the whole reason for per-process capture.
3. Kill the app mid-recording. The file must still play (R7).
4. Check levels are not attenuated or clipped against the source.

The full matrix is in [spec/test-plan.md](spec/test-plan.md). The four-hour
drift soak is the one that actually gates the build; do it once the basics work.

## Signing and notarization — later

Only needed to give the app to someone who is not you. Building and running
locally needs none of it. When it becomes relevant:
hardened runtime, sign, notarize, staple. Budget real time — it is usually more
work than the capture code.

## If you get stuck

Open an issue with what you tried and what happened. `spec/capture-macos.md` was
written without a Mac, so if it is wrong, that is expected — say so and it gets
fixed. Corrections to that file are as valuable as code.
