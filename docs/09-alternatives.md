# Alternatives — and when to just use one instead

An honest answer to "why not just use OBS?", because for part of this problem
the answer is **you should**.

## OBS Studio

OBS has **Application Audio Capture** on both target platforms — Windows
natively, and macOS 13+ with OBS 30+ — and can record each source to its own
track in Advanced output mode. Pointed at Discord, with your mic as a second
source, it produces exactly what DiscRec produces, using the same OS APIs.

It is free, mature, maintained by people who do this full-time, and works today.

### What OBS already does as well or better

| | |
|---|---|
| Per-application capture | Yes, both OSes. Same OS APIs underneath |
| Separate tracks per *source* | Yes — Discord, mic, game, browser |
| Recording reliability | Far more battle-tested than anything this project will produce |
| Go Live / stream audio | Yes — as does DiscRec, since both record what you hear |
| Video, if you ever want it | Yes. Explicitly a non-goal here |

### What OBS structurally cannot do

| | Why |
|---|---|
| **Open and record in one action** | A scene and a source must exist first. DiscRec has one button and no configuration (R13) |
| **Run in ~40 MB** | OBS is a video production suite. DiscRec targets under 40 MB and 3% CPU (R10, R11) |
| **Be obvious** | OBS assumes you know what a source is. This assumes nothing |

## The honest recommendation

**If you already have OBS configured and don't mind opening it: use OBS.**

DiscRec is not a new capability. It is the same OS capture API underneath, doing
one job, in a form you can hand to someone who has never configured a scene.
The value is entirely in setup cost and footprint — a ~200 MB production suite
versus a binary with one button.

That is a real product, but it is worth being clear about what it is. If OBS's
setup does not bother you, this offers you nothing.

The thing neither can do is per-person tracks, which need cryptographic
membership in the call. That route was specced and deliberately dropped as too
much setup for the premise — see [deferred/](deferred/README.md) and
[ADR-0008](adr/0008-manual-control.md).

## Other options considered

| Tool | Verdict |
|---|---|
| **Craig** (hosted) | The bot route, already built and run by someone else, with per-person tracks. If a bot in your server is acceptable, **Craig is strictly better than this project at what it does** — use it. DiscRec exists for people who want a local file and no setup |
| **Audacity / system loopback** | Works, entirely manual, whole-system audio. Strictly worse than OBS here |
| **Virtual audio cables** (VB-CABLE, BlackHole) | The pre-2021 approach. Unnecessary now that both OSes expose per-process capture ([C1](02-constraints.md)), and they fail R12 by requiring a driver |
| **Client mods / self-bots** | ToS violation and account termination, and they produce nothing that per-process capture doesn't already give you |
| **Phone recording next to speakers** | Only mobile option that exists. Terrible quality, but worth knowing it is the *only* thing that works on a phone ([C5](02-constraints.md)) |

## What this means for scope

Every feature request should be checked against this page. If the answer is
"OBS already does that, better", the feature does not belong here — adding it
moves DiscRec toward being a worse OBS, which is the one thing it must not
become.

The scope is: one button, one file, small. That is the whole differentiator.
