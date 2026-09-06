# Building DiscRec on a Mac

This is the Mac counterpart of the Windows app. The capture backend, Discord
process finder, AppKit window, bundle, and signing steps are **already in this
repository**. You are not being asked to design them.

On a Mac that meets the requirements below, this is the whole job:

```bash
git clone https://github.com/Sayandeep1013/DiscRec.git
cd DiscRec
bash scripts/macos/run.sh
```

That builds a release binary, wraps it in `dist/DiscRec.app`, ad-hoc signs it
(required for permission prompts), and opens it. Start Discord, join a call,
press Record, stop. The file lands in `~/Downloads/DiscRec/`.

If `run.sh` fails, use the troubleshooting section. Do not invent a different
capture API.

---

## 1. Machine

| Need | How to check | If it fails |
|---|---|---|
| macOS **14.2** or later (14.4+ nicer) | `sw_vers -productVersion` | The app will refuse to start capture with a clear error. Upgrade macOS. |
| Apple Silicon or Intel | either is fine; the script builds the native arch | Do not add a universal binary unless you have a reason. |
| Xcode Command Line Tools | `xcode-select -p` | `xcode-select --install` |
| Rust (stable, **1.85+**) | `rustc -V` | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` then open a new terminal |
| Discord desktop | — | Any client: stable, Canary, or PTB |

An Apple Developer account is **not** required to build, run, or record on this
Mac. It is only needed later if you want to give the `.app` to someone else
(notarization). Skip it.

Do not enable App Sandbox. Process taps do not work in the sandbox.

The Windows GNU-toolchain notes in `HANDOFF.md` do not apply. macOS uses clang
from the Command Line Tools.

---

## 2. What this repo already contains

| Path | Role |
|---|---|
| `src/capture/macos.rs` | Core Audio process tap (Discord) + default microphone |
| `src/discord.rs` | Finds Discord's **root** PID and the helper tree |
| `src/macos_ui.rs` | AppKit window: Record, meters, folder, tray while recording |
| `src/session.rs`, `mixer.rs`, `writer.rs` | Shared with Windows. Do not rewrite. |
| `macos/Info.plist` | Bundle id `com.discrec.app`, mic + system-audio usage strings |
| `macos/DiscRec.entitlements` | Microphone only. **No** `app-sandbox`. |
| `scripts/macos/run.sh` | Build, bundle, ad-hoc sign, launch |

The window is **not** Win32. `src/ui.rs` is Windows-only. On Mac, `lib.rs`
loads `macos_ui.rs` as the `ui` module.

---

## 3. How capture works (so you do not "fix" it into a system recorder)

Windows attaches to a PID with WASAPI process-tree loopback. macOS has no PID
loopback flag. The equivalent is:

1. Find Discord's root process (`Discord`, `Discord Canary`, or `Discord PTB`).
   Helpers are named `Discord Helper…` and are **children**. Audio is rendered
   by a child. Targeting only the root captures silence.
2. Collect every PID in that tree (`discord::descendant_pids`).
3. Translate each PID to a Core Audio process object
   (`kAudioHardwarePropertyTranslatePIDToProcessObject`). Retry for ~2 seconds
   if Discord has not registered with `coreaudiod` yet.
4. `CATapDescription::initStereoMixdownOfProcesses(those objects)`.
   `isPrivate = true`, `muteBehavior = Unmuted` (the user still hears the call),
   `processRestoreEnabled = true`.
5. **Never** `initStereoGlobalTapButExcludeProcesses([])` or `setExclusive(true)`
   with an empty process list. That is what `cpal`'s loopback does. It records
   **every app** and fails requirement R2 (music and games in the file).
6. Wrap the tap in a **private** aggregate device (`TapAutoStart`, tap UID in
   `TapList`). Wait until `kAudioDevicePropertyDeviceIsAlive` is set — starting
   IO before that yields digital silence with no error.
7. IOProc on the aggregate → `Frame { source: DiscordOutput, sample_pos from
   mSampleTime, … }` on the session channel. Do not block the IOProc on encode
   or disk.
8. Microphone is a **second** stream on the default input device. The existing
   mixer resamples it to Discord's clock. Do not stall the Mac port on
   aggregate drift-compensation experiments.

Quiet calls are valid recordings. Digital silence because the tap never
attached, or because TCC was denied, is not. There is no reliable API to query
system-audio TCC; a denied prompt looks like silence. The UI has **Open
Settings** when the error looks like a permission failure. Reset with:

```bash
tccutil reset AudioCapture com.discrec.app
tccutil reset Microphone com.discrec.app
```

Then launch `dist/DiscRec.app` again (not a raw `cargo run` binary — TCC is per
bundle id).

---

## 4. Commands

```bash
bash scripts/macos/run.sh          # the product
cargo test                         # shared mixer/writer tests
cargo run --release -- 30 --mix    # CLI, no window; writes mixed.ogg in cwd
```

`cargo run` without flags also opens the window, but **permission prompts for
an unsigned cargo binary are flaky**. Use the bundled app for real recordings.

WAV (`discrec 12` with no `--mix`) is Windows-only. On Mac always use `--mix`.

---

## 5. First-run checklist (human, two minutes)

1. `bash scripts/macos/run.sh`
2. Allow **Microphone** when asked.
3. Allow **System Audio Recording** / audio capture when asked.
4. Start Discord, join a voice channel, press Record, talk, stop.
5. Open folder — file is `~/Downloads/DiscRec/DiscRec-YYYY-MM-DD-HHMMSS.ogg`
6. Play it. Both sides should be there.
7. Play music on the Mac while recording a second clip. **The music must not
   be in the file.** If it is, the tap was created as a global tap; stop and
   fix `src/capture/macos.rs` — do not ship that.

If you dismissed a prompt: reset TCC as above, then run the **.app** again.

---

## 6. If something does not compile

The Mac code was written against `objc2` 0.6 / `objc2-core-audio` 0.3 from
Windows and compiled in GitHub Actions (`macos-15`). Apple and the crates
move. If a name changed:

- Keep the **behaviour** in section 3.
- Update only the symbol (`initStereoMixdownOfProcesses`, dictionary keys,
  `AudioDeviceCreateIOProcID`, AppKit `define_class!` syntax).
- Do not switch to ScreenCaptureKit, a Swift sidecar, `cpal` loopback, or
  `flexaudio`.
- Do not rewrite the mixer.

Useful references (read, do not copy their global-tap path):

- Apple: [Capturing system audio with Core Audio taps](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps)
- `CATapDescription::initStereoMixdownOfProcesses` in `objc2-core-audio`
- [AudioCap](https://github.com/insidegui/AudioCap) (Swift; aggregate-device step)
- cpal `loopback.rs` — **wrong process list** (exclusive + empty). Steal only
  the aggregate-device dictionary shape.

---

## 7. Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `run.sh`: needs 14.2 | Old OS | Upgrade |
| `dlltool` / GNU notes | Those are Windows | Ignore `HANDOFF.md` toolchain pages |
| App opens, Record does nothing useful, "Start Discord first." | Finder used a helper PID, or Discord not running | Start the **Discord** app, not a browser tab. Check Activity Monitor for `Discord`. |
| "no process object yet" | Discord has no Core Audio client yet | Join a voice channel, wait a second, Record again. |
| File exists, both sides missing / digital silence | TCC denied, or aggregate started before alive | Reset TCC, run the `.app`. Confirm `wait_until_alive` still exists in `macos.rs`. |
| Music/game in the file | Global tap | You used exclusive+empty. Revert to `initStereoMixdownOfProcesses`. |
| User cannot hear Discord while recording | Tap muted the process | `muteBehavior` must stay `Unmuted`. |
| Prompt never appears | Running unsigned `target/release/discrec` | Use `dist/DiscRec.app`. `Info.plist` must contain both usage strings. |
| `codesign` errors about sandbox | Entitlements gained `app-sandbox` | Remove it. |
| Mic missing, Discord present | Default input | Check System Settings → Sound → Input. |
| Window never appears | Not on main thread / no `NSApplication` | `macos_ui.rs` must call `NSApplication::run` on the main thread. |

---

## 8. Optional: give the app to someone else

Local ad-hoc signing (`codesign --sign -`) is enough on **this** Mac. To mail
the `.app` to another person you need an Apple Developer account (~$99/yr):

1. Hardened runtime: `codesign --options runtime --entitlements macos/DiscRec.entitlements --sign "Developer ID Application: …" dist/DiscRec.app`
2. Notarize: `xcrun notarytool submit …`
3. Staple: `xcrun stapler staple dist/DiscRec.app`

Do not start this until local recording already works. It is unrelated to
capture quality.

---

## 9. Decisions already made — do not relitigate

- Manual Record/Stop only ([ADR-0008](adr/0008-manual-control.md)).
- Native Core Audio taps, not Tauri, not Electron, not cpal loopback
  ([ADR-0007](adr/0007-cross-platform-strategy.md)).
- Native AppKit window, not egui/Slint ([ADR-0010](adr/0010-windows-native-shell.md)
  is Windows-specific; Mac mirrors the **product**, not Win32).
- One Ogg/Opus file. Mixer and writer stay shared.
- Quiet Discord is a valid recording. Wrong process / denied TCC is not.
- No bot, no auto-start, no mobile.

---

## 10. After it works

Push the branch. If you had to change a crate API, update this file and
`docs/spec/capture-macos.md` with what the hardware actually did (attenuation,
tap-on-restart). Those notes are useful; a second capture stack is not.
