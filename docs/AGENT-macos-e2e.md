# Agent E2E — macOS DiscRec

**You are an AI coding agent on a Mac.** Read this file to the end before
running commands or editing code. Then execute the jobs **in order**. Stop on
FAIL unless the job says how to recover.

This is not a design brief. The Mac app, capture backend, AppKit window,
bundle, and signing script are already in the repository and already compile
on GitHub Actions (`macos-15`). Your job is to **build it on this Mac, get
permissions right, prove a real recording, and report**.

Human playbook (same facts, less procedure): [CONTRIBUTING-macos.md](CONTRIBUTING-macos.md).
Capture behaviour (do not change): [spec/capture-macos.md](spec/capture-macos.md).

Repo: `https://github.com/Sayandeep1013/DiscRec`

---

## 0. Who you are and what you must not do

DiscRec records **Discord's audio + the default microphone** into one Ogg/Opus
file when the user presses Record. Manual control only. No bot, no auto-start,
no Electron, no sandbox.

**Do not:**

- Switch capture to ScreenCaptureKit, `cpal` loopback, `flexaudio`, a Swift
  sidecar, or a global / exclusive-empty process tap.
- Rewrite `src/mixer.rs`, `src/session.rs`, or `src/writer.rs`.
- Add App Sandbox to `macos/DiscRec.entitlements`.
- Treat `HANDOFF.md` GNU/`dlltool`/`D:` notes as applying here. Those are
  Windows. This Mac uses clang from Xcode Command Line Tools.
- Ship a recording that contains Music.app / YouTube / games. That means the
  tap is system-wide (R2 fail).
- Abort a recording because the call is quiet. Quiet is valid. Digital
  silence from a denied permission or a dead tap is not.
- Test recordings with `cargo run`. TCC follows **`dist/DiscRec.app`**, bundle
  id `com.discrec.app`.

**You may edit** only to make the existing design compile or to fix a proven
Mac bug: `src/capture/macos.rs`, `src/macos_ui.rs`, `src/discord.rs`,
`src/paths.rs`, `macos/Info.plist`, `macos/DiscRec.entitlements` (mic
entitlement only), `scripts/macos/run.sh`. If a crate symbol moved, rename it.
Keep the behaviour in [spec/capture-macos.md](spec/capture-macos.md).

---

## 1. Human gates (you cannot click these)

Ask the human once, up front, and wait:

1. This Mac is macOS **14.2+**.
2. They will **Allow** Microphone and System Audio Recording when macOS asks.
3. Discord **desktop** (not a browser tab) can join a voice channel with them
   talking.
4. They can play music on the Mac in a **second** test (Music.app or a browser
   that is not Discord).

If they refuse permissions, Job 8 (denied TCC) is still useful; Jobs 6–7
cannot pass.

---

## 2. Jobs

Copy the **Report** block at the bottom into your final message when you stop.

### Job A — Machine

```bash
uname -s
sw_vers -productVersion
uname -m
xcode-select -p
rustc -V
cargo -V
```

| Check | Pass |
|---|---|
| `uname -s` | `Darwin` |
| macOS | `14.2` or later (`15.x` / `26.x` fine) |
| `xcode-select -p` | a path, not an error |
| `rustc -V` | **1.85+** (stable) |

**Recover:**

- Not Darwin → stop. This file is Mac-only.
- macOS &lt; 14.2 → stop. Tell the human to upgrade.
- No CLT → `xcode-select --install`, then wait for the human to finish the GUI
  installer. Do not loop.
- No Rust / too old →

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
  rustup update stable
  rustc -V
  ```

  Ask the human to open a new terminal if `cargo` is still missing.

---

### Job B — Tree

If this is not already the DiscRec repo:

```bash
git clone https://github.com/Sayandeep1013/DiscRec.git
cd DiscRec
```

If it is:

```bash
git pull --ff-only origin main
git log -1 --oneline
```

Confirm these paths exist:

- `src/capture/macos.rs`
- `src/macos_ui.rs`
- `macos/Info.plist`
- `macos/DiscRec.entitlements`
- `scripts/macos/run.sh`

```bash
grep -n app-sandbox macos/DiscRec.entitlements || true
plutil -lint macos/Info.plist
```

**Fail** if entitlements contain `app-sandbox`. Remove it. Process taps do not
work sandboxed.

**Fail** if Info.plist lacks `NSMicrophoneUsageDescription` or
`NSAudioCaptureUsageDescription`.

---

### Job C — Unit tests (no Discord)

```bash
cd "$(git rev-parse --show-toplevel)"
cargo test
```

**Pass:** all tests green. These are shared mixer/writer tests; they do not
prove capture.

**Fail:** fix compile errors only as allowed in section 0. Re-run `cargo test`.
Do not skip tests with `--lib` to hide a binary failure.

---

### Job D — Product bundle

```bash
SKIP_OPEN=1 bash scripts/macos/run.sh
```

**Pass:**

- `dist/DiscRec.app/Contents/MacOS/DiscRec` exists and is executable
- `codesign --verify --verbose=2 dist/DiscRec.app` succeeds (ad-hoc is OK)
- `codesign -d --entitlements :- dist/DiscRec.app` has **no** `app-sandbox`
- `plutil -p dist/DiscRec.app/Contents/Info.plist` shows bundle id
  `com.discrec.app` and both usage strings
- `otool -L dist/DiscRec.app/Contents/MacOS/DiscRec` links system frameworks,
  not a mystery sidecar

**Fail:** fix `scripts/macos/run.sh` or the plist/entitlements. Do not invent a
`.pkg` or notarization flow.

---

### Job E — Launch the real app (TCC)

```bash
open dist/DiscRec.app
```

Ask the human: **Allow Microphone. Allow System Audio Recording.** Both.

If they already dismissed a prompt:

```bash
tccutil reset AudioCapture com.discrec.app
tccutil reset Microphone com.discrec.app
open dist/DiscRec.app
```

**Pass:** window titled DiscRec appears (first run may show a recording
notice — Continue is expected).

**Fail:** window never appears. Check `macos_ui.rs` still calls
`NSApplication::run` on the main thread. Do not “fix” it by switching to
`cargo run`.

---

### Job F — Discord is findable

Human starts **Discord.app** (stable, Canary, or PTB).

```bash
pgrep -fil 'Discord' | head -40
```

**Pass:** a process named `Discord` / `Discord Canary` / `Discord PTB` exists.
Helpers (`Discord Helper…`) are children; that is normal.

**Fail:** only a browser. Tell the human to start the desktop app.

Optional, from the **bundled** binary (uses the same code as Record):

```bash
# no-op if Discord is down — it should print the root pid when Discord is up
dist/DiscRec.app/Contents/MacOS/DiscRec 1 --mix || true
```

A 1-second mix that exits `1` with `Discord isn't running` is a **pass** for
this job only if `pgrep` also found nothing. If Discord is running and the
binary still says it is not, fix `src/discord.rs` (`DISCORD_PROCESSES`,
`descendant_pids`, `proc_name` matching). Do not match on window title.

---

### Job G — Mixed recording (the product)

Human: join a **voice channel**, talk, keep the mic unmuted.

From the repo root, using the **signed app binary** (not `cargo run`):

```bash
rm -f mixed.ogg
dist/DiscRec.app/Contents/MacOS/DiscRec 20 --mix
ls -l mixed.ogg
```

**Pass:**

- Process exits 0
- stdout contains `BOTH SIDES CAPTURED` (or both Discord peak and mic peak
  well above `1e-6`)
- `mixed.ogg` exists and is not tiny empty (tens of KB+ for 20 s is typical)
- Human plays `mixed.ogg` and hears **call audio and their voice**

If stdout says `No Discord audio` while people were talking:

1. Confirm they joined a voice channel before Record (Core Audio process
   objects appear when Discord has an audio client).
2. Reset TCC (Job E) and retry **from the .app binary**.
3. Confirm `src/capture/macos.rs` still uses
   `initStereoMixdownOfProcesses` on **descendant** process objects, waits
   until the aggregate is alive, and does **not** use exclusive + empty list.

If Discord is present and mic is `1e-6`: System Settings → Sound → Input.
Default input is what we capture.

Also exercise the **window** once (human): Continue if notice, Record ~15 s,
Stop. File at `~/Downloads/DiscRec/DiscRec-YYYY-MM-DD-HHMMSS.ogg`. Open
folder in the app must open that directory.

**Pass (GUI):** that file plays with both sides.

WAV without `--mix` is Windows-only. On Mac always `--mix`.

---

### Job H — Isolation (R2) — music must not be in the file

Human: start **music on the Mac** that is not Discord (Music.app, YouTube in
Safari/Chrome). Stay in the Discord call. Record 15–20 s while music is
clearly audible on the speakers.

```bash
rm -f mixed-isolation.ogg
dist/DiscRec.app/Contents/MacOS/DiscRec 18 --mix
mv -f mixed.ogg mixed-isolation.ogg
```

Human plays `mixed-isolation.ogg`.

| Heard in file | Result |
|---|---|
| Discord + mic, **no** Music/YouTube | **PASS** |
| Music/YouTube clearly in the mix | **FAIL R2** |

On FAIL R2: you used a global tap. Revert any change that calls
`initStereoGlobalTapButExcludeProcesses`, `setExclusive(true)` with an empty
process list, or an empty `processes` array. Restore
`initStereoMixdownOfProcesses` on Discord's process-object list. Re-run Job H.
Do not ship.

---

### Job I — Denied permission is not a silent file (R8)

Only after Jobs G–H passed, so you know capture can work.

```bash
tccutil reset AudioCapture com.discrec.app
tccutil reset Microphone com.discrec.app
```

Human: launch `dist/DiscRec.app` (or the CLI below) and click **Don't Allow**
on at least the system-audio prompt.

```bash
dist/DiscRec.app/Contents/MacOS/DiscRec 8 --mix
```

**Pass:** non-zero exit and an error about permission / capture failure —
**not** a healthy `BOTH SIDES CAPTURED` with a silent file. The window should
offer **Open Settings** on a permission-shaped error.

**Then restore Allow** (reset TCC again, open the app, Allow both) so the
human is not stuck.

Quiet Discord with permissions granted is **not** this test. Do not treat
“nobody spoke” as `NoSignal`.

---

### Job J — Hardware notes (fill spec, do not redesign)

After a passing Job G, append measured facts to
[spec/capture-macos.md](spec/capture-macos.md) under **Hardware notes**:

1. Was the tap quieter than Discord in the headset? Rough dB / “about half”
   is enough.
2. Sample rate printed or inferred (48 kHz vs 44.1 kHz). Session already
   resamples.
3. Anything surprising (tap died when Discord relaunched, TCC prompt only on
   first `.app` launch, etc.).

Do not start notarization or a Developer ID cert. Ad-hoc is enough on this
Mac.

---

## 3. If `cargo` / `run.sh` does not compile

GitHub Actions already compiled this tree on `macos-15` with `objc2` 0.6 /
`objc2-core-audio` 0.3. If **this** Mac fails:

1. Keep behaviour in [spec/capture-macos.md](spec/capture-macos.md).
2. Change only the broken symbol or feature flag.
3. Re-run Job C then Job D.
4. Do not add new capture crates.

References (read; do not copy cpal's exclusive+empty tap):

- Apple: Capturing system audio with Core Audio taps
- `CATapDescription::initStereoMixdownOfProcesses`
- AudioCap (Swift) for the aggregate-device step only

---

## 4. Commands cheat sheet

```bash
bash scripts/macos/run.sh                 # build, sign, open
SKIP_OPEN=1 bash scripts/macos/run.sh     # build, sign, do not open
cargo test
dist/DiscRec.app/Contents/MacOS/DiscRec 20 --mix
tccutil reset AudioCapture com.discrec.app
tccutil reset Microphone com.discrec.app
```

Binary for all capture tests: `dist/DiscRec.app/Contents/MacOS/DiscRec`.

---

## 5. Report (paste this when you stop)

```text
macOS:                 (sw_vers)
arch:                  (uname -m)
rustc:                 (rustc -V)
git:                   (git log -1 --oneline)
Job C cargo test:      PASS / FAIL
Job D bundle+sign:     PASS / FAIL
Job E window+TCC:      PASS / FAIL (Allowed both? Y/N)
Job F Discord found:   PASS / FAIL
Job G mix both sides:  PASS / FAIL (peaks / BOTH SIDES CAPTURED)
Job G GUI file:        PASS / FAIL / skipped
Job H no music in file: PASS / FAIL
Job I deny ≠ silence:  PASS / FAIL / skipped
Code changes:          none / list files
Hardware notes:        (or "none yet")
Blockers for human:    (permissions, Discord, OS, none)
```

If Jobs C–H are PASS, the Mac port is proven on this machine. Push only if
the human asked you to, and only the compile/spec notes you actually needed.
Notarization is out of scope.
