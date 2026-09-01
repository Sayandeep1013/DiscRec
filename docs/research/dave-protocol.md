# Research — DAVE, Discord's voice E2EE

**Checked: 1 Sept 2026.** Library facts here age fastest; re-verify versions
before depending on them.

## What it is

DAVE (Discord Audio & Video End-to-End Encryption) is Discord's E2EE protocol
for voice and video, introduced Sept 2024. Call participants form an **MLS**
(Messaging Layer Security) group; the voice gateway acts as delivery and
authentication service. Members export **per-sender ratcheted media keys**, with
a new ratchet per MLS epoch. Discord holds no media keys.

## Timeline

| Date | Event |
|---|---|
| Sept 2024 | Protocol introduced, whitepaper and `libdave` published |
| 2025 | Extended to browsers, consoles, bots, apps, Social SDK |
| **2 Mar 2026** | **Enforced.** Clients without DAVE cannot connect — close code `4017` |
| 18 May 2026 | Discord announces E2EE complete for all voice and video calls |

## What it means for recording

**Interception is dead.** The wire carries ciphertext Discord's own servers
cannot read. Any proxy or network-tap design is permanently non-viable.

**Bots are not excluded.** A bot implementing DAVE joins the MLS group as a
member and derives per-sender keys — it can decrypt every sender, because that
is what call membership means. The whitepaper does not carve bots out; it
describes downgrade behaviour for clients that do not *support* the protocol,
and since March those are simply rejected rather than downgraded.

**Metadata is explicitly out of scope** of the threat model: participants,
duration, and usage patterns are observable by the service.

## Library support — the important part

| Library | DAVE status | Verdict |
|---|---|---|
| **`@snazzah/davey`** | Full DAVE implementation, OpenMLS-based, MIT. **npm 0.1.12** (published 22 Jun 2026), also **crates.io** (Rust) and **PyPI** (Python) | **Use this.** The only working non-Discord implementation found |
| `@projectdysnomia/dysnomia` | Voice connection exposes `daveSession`, `daveProtocolVersion`, `lastTransitionID`, `transitioned` event | Proven in production by Craig |
| `@discordjs/voice` 0.19.x | DAVE on by default; **receive broken** — reconnect loops, `DecryptionFailed(UnencryptedWhenPassthroughDisabled)`, zero audio captured. Open issue, no maintainer fix seen | Do not use for receive |
| py-cord 2.8.0 (18 May 2026) | Changelog: *"support for Discord DAVE … for voice-sending related features"* — **sending only** | Not sufficient for a recorder |
| `discord-ext-voice-recv` | Most mature discord.py receive extension; **no DAVE mention** in its docs | Verify directly before relying on it |

The initial assessment for this project concluded Route A was blocked because no
library shipped DAVE receive. That was true of the mainstream libraries and
**false of the ecosystem** — see [prior-art-craig.md](prior-art-craig.md).

## Operational cost

MLS re-keys on every membership change. Each join or leave triggers an epoch
transition, and transitions can fail, producing undecryptable audio. Craig
carries an explicit `encryptionRecoveryAttempts` counter and recovery path,
which is strong evidence this happens in production rather than only in theory.
→ [P3](../05-challenges.md#p3)

## Protocol details worth knowing

- Identify (op 0) carries `max_dave_protocol_version`.
- Session description (op 4) returns the negotiated `dave_protocol_version`.
- Ops 21–31 handle transitions, MLS group management, key packages, commits.
- Transport encryption: must support `aead_xchacha20_poly1305_rtpsize`; prefer
  `aead_aes256_gcm_rtpsize`. The older `xsalsa20_poly1305` modes were removed in
  Nov 2024.

## Sources

- Whitepaper — https://daveprotocol.com/ and https://github.com/discord/dave-protocol
- `libdave` (Discord's own) — https://github.com/discord/libdave
- `davey` — https://github.com/Snazzah/davey
- Voice docs — https://docs.discord.com/developers/topics/voice-connections
- discord.js issue #11419 — https://github.com/discordjs/discord.js/issues/11419
- py-cord issue #3135 — https://github.com/Pycord-Development/pycord/issues/3135
- Discord announcement — https://discord.com/blog/every-voice-and-video-call-on-discord-is-now-end-to-end-encrypted
