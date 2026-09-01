# Spec — Configuration

There is almost none, deliberately. Zero setup is a `MUST` requirement (R13),
and every setting is a decision pushed onto someone who wanted to press record.

## Defaults that need no configuration

| Behaviour | Default | Why not configurable |
|---|---|---|
| Recording location | `Documents/DiscRec/` | Changeable via the one setting below |
| Format | Ogg/Opus 48 kHz stereo ~96 kbps | A format picker is a question nobody benefits from answering |
| Discord instance | Auto — stable, Canary or PTB, whichever has audio | Detected reliably |
| Microphone | System default input | Follows the device the user already chose in the OS |
| File naming | Timestamp | Nothing else is available; the app does not know the channel |

## The settings that exist

```toml
[general]
storage_dir   = "..."     # default: Documents/DiscRec
include_mic   = true      # false records Discord's side only

[audio]
mic_gain_db   = 0.0       # trim only, for a mic that is hot or quiet
```

Three values. `include_mic` exists because a one-sided recording is occasionally
what someone wants; `mic_gain_db` exists because mixing is irreversible
([P4](../05-challenges.md#p4)) and a badly-matched microphone otherwise has no
remedy.

Edited in the app, stored at:

| OS | Path |
|---|---|
| Windows | `%APPDATA%\DiscRec\config.toml` |
| macOS | `~/Library/Application Support/DiscRec/config.toml` |

## Deliberately absent

- **Output device / input device pickers.** The OS already has these.
- **Bitrate, sample rate, codec.** One correct answer, chosen.
- **Auto-start** ([ADR-0008](../adr/0008-manual-control.md)).
- **Telemetry opt-in.** There is no telemetry to opt into (R16).
- **Anything that suppresses the recording reminder**
  ([06-legal-and-consent.md](../06-legal-and-consent.md)).

## Behaviour with no config file

The app must work correctly having never written one. The file is created only
when a value is changed from its default. An invalid file is reported with the
specific key and accepted range, and the app continues on defaults rather than
refusing to start — with three settings, none is important enough to block on.
