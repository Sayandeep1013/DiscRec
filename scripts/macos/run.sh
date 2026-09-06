#!/usr/bin/env bash
# Build DiscRec.app, ad-hoc sign it, and launch it.
# Run from anywhere:  bash scripts/macos/run.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script only runs on a Mac." >&2
  exit 1
fi

VER="$(sw_vers -productVersion)"
MAJOR="${VER%%.*}"
REST="${VER#*.}"
MINOR="${REST%%.*}"
if (( MAJOR < 14 || (MAJOR == 14 && MINOR < 2) )); then
  echo "DiscRec needs macOS 14.2 or later. This Mac is ${VER}." >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is not installed. Run: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
  exit 1
fi

if ! xcode-select -p >/dev/null 2>&1; then
  echo "Xcode Command Line Tools are missing. Run: xcode-select --install" >&2
  exit 1
fi

echo "Building release…"
cargo build --release

APP="$ROOT/dist/DiscRec.app"
BIN_SRC="$ROOT/target/release/discrec"
if [[ ! -x "$BIN_SRC" ]]; then
  echo "cargo did not produce target/release/discrec" >&2
  exit 1
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_SRC" "$APP/Contents/MacOS/DiscRec"
chmod +x "$APP/Contents/MacOS/DiscRec"
cp "$ROOT/macos/Info.plist" "$APP/Contents/Info.plist"

if [[ -f "$ROOT/assets/icon.png" ]] && command -v sips >/dev/null && command -v iconutil >/dev/null; then
  ICONSET="$(mktemp -d)/DiscRec.iconset"
  mkdir -p "$ICONSET"
  for sz in 16 32 64 128 256 512; do
    sips -z "$sz" "$sz" "$ROOT/assets/icon.png" --out "$ICONSET/icon_${sz}x${sz}.png" >/dev/null
    dbl=$((sz * 2))
    sips -z "$dbl" "$dbl" "$ROOT/assets/icon.png" --out "$ICONSET/icon_${sz}x${sz}@2x.png" >/dev/null
  done
  iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/DiscRec.icns"
  /usr/libexec/PlistBuddy -c 'Add :CFBundleIconFile string DiscRec' "$APP/Contents/Info.plist" 2>/dev/null || true
fi

# Ad-hoc sign. Required for TCC prompts (microphone + system audio).
# Do not enable App Sandbox — process taps are unavailable to sandboxed apps.
codesign --force --deep --sign - \
  --entitlements "$ROOT/macos/DiscRec.entitlements" \
  "$APP"

echo
echo "App: $APP"
if [[ "${SKIP_OPEN:-}" == "1" || "${CI:-}" == "true" ]]; then
  echo "Skipping launch (CI). Binary is signed at $APP"
  codesign --verify --verbose=2 "$APP" || true
  exit 0
fi

echo "Opening. macOS will ask for Microphone and System Audio Recording."
echo "Allow both. If you click Don't Allow, the recording will be silent —"
echo "fix that with:  tccutil reset AudioCapture com.discrec.app"
echo "            and: tccutil reset Microphone com.discrec.app"
echo
open "$APP"
