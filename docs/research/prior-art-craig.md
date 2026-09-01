# Research — Craig, and how it solved DAVE

**Checked: 1 Sept 2026** by reading the repository directly.

Craig (https://craig.chat, https://github.com/CraigChat/craig) is an open-source
multi-track Discord voice recorder, by Snazzah. It is **the** reference
implementation for this problem and was actively committed to on the day this
was checked.

## Why it matters here

It answers the question that blocked the whole project: *can a bot still record
after DAVE enforcement?* Yes — and the mechanism is public.

## The stack, from `apps/bot/package.json`

| Dependency | Role |
|---|---|
| `@snazzah/davey` ^0.1.12 | DAVE protocol — same author as Craig |
| `@projectdysnomia/dysnomia` (pinned commit) | Eris fork; DAVE-aware voice connection |
| `@discordjs/opus` ^0.10.0 | Opus, for processing rather than the capture path |
| `sodium-native` ^5.1.0 | Transport encryption |

Notably **not** discord.js — consistent with `@discordjs/voice` having DAVE
receive broken.

## Observed implementation details

From `apps/bot/src/modules/recorder/recording.ts`:

**DAVE session handling.** A `transitioned` event carries a transition ID; the
handler logs it with `daveProtocolVersion` and the resulting user list from
`session.getUserIds()`. There is a separate **encryption recovery** path with an
attempt counter, logging endpoint, transition ID, `reinitializing` state and
channel ID. This is engineered-for, not incidental — transitions fail in
production.

**Receive.** `connection.receive('opus')` yields a stream whose `data` events
carry `(buffer, userID, timestamp)`.

**Ordering.** Frames are pushed into a per-user array with both the RTP
`timestamp` and a monotonic `time` derived from `process.hrtime`. When an
incoming timestamp is older than the buffer's tail, the buffer is **sorted** —
reordering is treated as normal.

**Two clocks, deliberately.** RTP timestamp for position, `hrtime` alongside.
This is the pattern adopted in
[ADR-0005](../adr/0005-timeline-dual-clock.md).

**Write path.** `encodeChunk(user, streamNo, packetNo, chunk)` →
`writer.writeChunk(...)`. Packet numbers advance by 2 per chunk, and a periodic
branch fires every 50 packets (`packetNo % 50 === 49`) — a bounded flush cadence
rather than a syscall per 20ms frame. Adopted in
[storage-format.md](../spec/storage-format.md).

## What to take, and what not to

**Take:** the dependency set, the dual-clock timeline, the reorder-and-sort
buffer, the periodic flush cadence, and the fact that encryption recovery needs
explicit handling.

**Do not take:** the architecture wholesale. Craig is a hosted multi-tenant
service with a job queue, a web frontend, a database and a separate "kitchen"
processing pipeline. DiscRec is a single-user local recorder. Most of Craig's
complexity is service complexity that does not apply.

## Use as an early-warning system

Bot voice receive is undocumented and can break without notice
([P8](../05-challenges.md#p8)). Craig is well-maintained and heavily used; it
will notice a platform break before we do. Watching its commits and issues is
cheap monitoring.

## Licensing

Check Craig's license before copying code rather than approach.
`@snazzah/davey` is **MIT**, which is unambiguous for use as a dependency.
