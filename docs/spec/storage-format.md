# Spec — Storage format

Satisfies R7, R14.

## Design goals

1. **A killed process leaves a playable file** (R7).
2. **Readable in ten years** — one open-format file, no sidecar index required
   to play it.
3. **Obvious on disk.** A folder of audio files a person can browse.

## Layout

```
Documents/DiscRec/
  2026-09-01_19-30-14.ogg
  2026-09-01_21-05-02.ogg
  2026-09-02_14-11-47.ogg
```

One file per session. No database, no manifest required for playback — losing
DiscRec entirely must not make the recordings less useful.

A sidecar `.json` is written alongside for diagnostics only
([diagnostics.md](diagnostics.md)). Deleting it costs nothing but the log.

## Container

**Ogg/Opus, 48 kHz, stereo, ~96 kbps.**

Opus because it is what voice is for, it is open, and it is small — roughly
43 MB per hour at 96 kbps. Ogg because it is resilient: a truncated file is
simply a file with a truncated last page, and decoders handle that.

Encoding happens once, from the mixed f32 stream. There is no passthrough path —
the OS delivers PCM, unlike a bot which receives Opus already encoded
([ADR-0004](../adr/0004-storage-opus-passthrough.md)).

## Crash safety

The requirement is that `SIGKILL` at any moment leaves playable audio.

- Flush Ogg pages roughly every second. That bounds loss without a syscall per
  20 ms frame.
- **No back-patched header.** Nothing may require rewriting the file start at
  the end, because a crash is precisely when that does not happen. Ogg's
  structure permits this; do not add anything that breaks it.
- The sidecar is written incrementally and flushed on state changes.

Verified by killing the process at 100 random offsets and asserting every
resulting file decodes (R7).

## Naming

`YYYY-MM-DD_HH-MM-SS.ogg`, local time. Sorts chronologically, contains no
channel name — the app never talks to Discord and does not know it
([ADR-0008](../adr/0008-manual-control.md)).

Collisions get a `_2` suffix rather than overwriting. Never overwrite a
recording.

## Disk space

~43 MB per hour. A weekly two-hour call is ~4 GB a year.

No automatic deletion in v1. A recorder that silently removes recordings is
worse than one that fills a disk, and the folder is plain enough to manage by
hand. The app warns when the target volume drops below 2 GB free, and refuses to
start a recording below 500 MB rather than failing partway through.

## Playback and editing

The output opens directly in VLC, Audacity, Reaper, and ffmpeg. No export step
exists in v1 — if a different format is needed, ffmpeg converts it in one
command, and shipping a transcoding UI for that would contradict the entire
premise of the app.
