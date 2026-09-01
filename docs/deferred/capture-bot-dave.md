# Spec — Route A: bot capture under DAVE

Satisfies R5. Addresses [P2](../05-challenges.md#p2),
[P3](../05-challenges.md#p3), [P8](../05-challenges.md#p8).

## Summary

A self-hosted bot joins the voice channel as a full participant, becomes a
member of the call's MLS group, derives per-sender media keys, and receives one
Opus stream per speaker. Packets are written through to disk without decoding
([ADR-0004](../adr/0004-storage-opus-passthrough.md)).

This is possible because a bot in a call is a legitimate call member — DAVE
encrypts *to the participants*, and the bot is one.

## Stack

| Concern | Choice | Why |
|---|---|---|
| DAVE / MLS | `@snazzah/davey` | MIT, OpenMLS-based, published for Node, Rust and Python. The only working DAVE implementation found outside Discord's own clients. |
| Voice gateway | `@projectdysnomia/dysnomia` | Has DAVE support in `VoiceConnection`; the stack Craig runs in production |
| Opus | `@discordjs/opus` | Only needed for export and diagnostics — not on the capture path |
| Crypto | `sodium-native` | Transport-layer encryption modes |

Mainstream libraries are **not** usable here as of Sept 2026: `@discordjs/voice`
0.19.x has DAVE receive broken in the open, and py-cord 2.8.0 shipped DAVE for
sending only. Verify both before assuming otherwise —
→ [research/dave-protocol.md](../research/dave-protocol.md)

## Connection lifecycle

```
identify (op 0)  ──▶ include max_dave_protocol_version
session description (op 4) ──▶ server returns dave_protocol_version
DAVE handshake (op 21–31) ──▶ MLS key packages, commits, welcome
        │
        ├─▶ transition(id) ─── every membership change re-keys the group
        │                      log id, protocol version, member list
        └─▶ receive ─────────▶ per-SSRC Opus frames
```

Required transport encryption modes: `aead_xchacha20_poly1305_rtpsize` must be
supported; prefer `aead_aes256_gcm_rtpsize` where available.

## Receive path

Model it on the reference implementation, which is known to work:

1. Open an Opus receive stream on the voice connection. Frames arrive with a
   user ID and a timestamp.
2. Push each frame into a **per-user buffer**. If the incoming timestamp is
   older than the buffer's tail, sort — reordering is normal, not an error.
3. Map SSRC to user via the gateway's Speaking event, buffering frames for
   unknown SSRCs briefly rather than discarding them
   ([P2](../05-challenges.md#p2)).
4. Emit `Frame::Opus { track, rtp_ts, seq, data }` to the timeline writer.
   Position comes from the RTP timestamp, never arrival
   ([ADR-0005](../adr/0005-timeline-dual-clock.md)).
5. Flush periodically rather than per packet — the reference flushes roughly
   every 50 packets, which bounds loss on crash (R7) without one syscall per
   20ms frame.

## Epoch transitions and encryption recovery

**Expect these to fail sometimes.** MLS re-keys on every join and leave, and the
reference implementation carries an explicit recovery-attempt counter — that
tells you it happens in production, not just in theory.

Required handling:

- Log every transition with its ID, the negotiated protocol version, and the
  resulting member list.
- On decryption failure: re-initialize the DAVE session and reconnect. **Do not
  end the recording** — R2 requires the session survive reconnects.
- Record a marker in the manifest for any span where decryption failed, so a
  gap in the audio has an explanation instead of being mistaken for silence.
- Bound the recovery attempts. Escalate to the user rather than looping forever.

## Auto-start

Free on this route. Subscribe to `VOICE_STATE_UPDATE`; when the configured user
ID enters a voice channel, join and start. Works regardless of which machine the
user joined from, and survives them switching machines mid-call.

## Consent

The strongest route for consent. The bot is visible in the member list, posts an
announcement on start, and exposes an opt-out command that genuinely removes a
user's frames — the only route where opt-out is implementable at all.
→ [../06-legal-and-consent.md](../06-legal-and-consent.md)

## Limits

- Requires permission to invite a bot. No DMs, no group calls.
- **No Go Live or screenshare audio, ever** ([C9](../02-constraints.md)).
  Detect streaming and warn (R20, [P10](../05-challenges.md#p10)).
- Records what the bot receives, which is not necessarily what you heard.
- Depends on undocumented receive behaviour ([C6](../02-constraints.md)) — hence
  Route B as a real fallback, not a theoretical one.

## Open questions

- Does the Rust `davey` crate expose the same surface as the Node package? The
  project's own README notes Rust documentation is thin. Resolve before
  committing to a Rust bot ([ADR-0002](../adr/0002-language-and-runtime.md)).
- Bot-account rate limits on rapid join/leave cycles when a user hops channels.
- Behaviour when two recorders are in one channel.
