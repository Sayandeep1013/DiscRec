# Spec — Bot deployment

Needed from Phase 0. Route A only.

## What has to run somewhere

Route A's recorder is a long-lived process holding a gateway connection. It must
be running *before* you join a voice channel, or the `VOICE_STATE_UPDATE` that
triggers auto-start is missed (R1).

That is the one real cost of the bot route, and it should be decided in Phase 0
rather than discovered in Phase 3.

## Options

| Where | Cost | Trade |
|---|---|---|
| **The same desktop** | None | Only records while that machine is on. Undermines much of the route's value, but is the fastest way to start. |
| **A home box / Raspberry Pi** | Hardware you may own | Always on, no monthly bill, no third party holding call audio |
| **A small VPS** | ~$5/mo | Always on, but call audio lands on someone else's disk — weigh against R14's intent |

**Recommendation: desktop for Phases 1–3, then a home box.** Phase 1 is about
proving DAVE receive works; hosting is a distraction until it does.

Note the tension with R14 ("recordings stay local"): a VPS is not local. It does
not violate the letter of R14, which is about the *product* not exfiltrating
data, but it does move audio off your machine. If you go that route, say so in
the consent announcement.

## Configuration

The bot needs, at minimum:

```
DISCREC_BOT_TOKEN       Discord bot token — secret
DISCREC_USER_ID         whose voice-channel joins trigger recording
DISCREC_GUILD_ALLOW     optional allowlist of guild IDs
DISCREC_STORAGE_DIR     where sessions are written
```

Token handling: environment or a mode-0600 file, never a config file committed
to the repo. `.gitignore` already excludes `bot-token*` and `.env`.

## Discord application setup

1. Create an application and a bot user in the Discord developer portal.
2. Enable the **Server Members** and **Message Content** intents only if the
   consent flow needs them; prefer the narrowest set that works.
3. Invite with the minimum permissions: View Channel, Connect, Speak (needed to
   be present, not to transmit), and Send Messages in the announcement channel.
4. **Do not** grant Administrator.

## Restart behaviour

The bot will restart — deploys, crashes, reboots. It must not lose an
in-progress recording (R7).

- On start, scan the storage directory for sessions whose manifest has
  `ended_at: null`. Those died mid-recording.
- Do **not** silently resume them into the same files. Finalize them as
  recovered, note the truncation in the manifest, and start a new session if the
  user is still in a voice channel.
- Resuming into a file whose write position is unknown risks corrupting audio
  that is otherwise intact.

## Process supervision

Whatever runs it must restart it. `systemd` on Linux hosts, a Windows service or
Task Scheduler entry on desktop, `launchd` on macOS. Restart with backoff — a
bot that crash-loops against a Discord rate limit will get its token flagged.

## Open questions

- Rate limits on rapid join/leave when hopping channels
  ([capture-bot-dave.md](capture-bot-dave.md)).
- Whether one bot instance can record two guilds concurrently, or whether that
  needs separate voice connections and separate storage roots.
