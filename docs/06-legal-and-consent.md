# Recording other people

Short, because manual control removes most of the problem — but not the
obligation.

## What changed

An auto-starting recorder records people who did not initiate it, possibly
without the operator present. That demanded announcement mechanics, opt-out
handling, and audit trails.

Pressing a button is different. You are there, you chose it, you know it is
happening. The design burden drops to almost nothing. **The legal obligation
does not.**

## Two obligations, both yours

**1. Discord's Terms** require notifying participants before recording, and
prohibit using others' content without consent or another lawful basis. Breach
risks account suspension regardless of local law.

**2. Recording law**, which varies:

| Regime | Requirement |
|---|---|
| All-party consent (CA, IL, WA, PA, FL, much of the EU) | Everyone must consent |
| One-party consent (much of the US and elsewhere) | You suffice |
| GDPR (any EU participant) | A lawful basis under Art. 6; voice is personal data |

Discord calls routinely cross jurisdictions and you rarely know where people
are. **Assume the strictest case** — you cannot determine it at runtime, and
guessing wrong is the expensive outcome.

## What the app can and cannot do

**Cannot:** announce anything. DiscRec never connects to Discord — it captures
audio from the OS. Nothing on Discord's side knows it exists. There is no bot in
the member list, no message, no indicator to anyone else.

**Can:** make it hard to forget. On first run, and thereafter when a recording
starts, the app states plainly that everyone in the call needs to be told. Not a
consent checkbox that trains people to click through — a short, honest line at
the moment it matters.

**Cannot honor an opt-out.** One mixed track cannot have a person removed. If
someone objects mid-call, the only real options are to stop recording or delete
the file. The app offers both; it will not pretend to mute someone
([P4](05-challenges.md#p4)).

## Deliberately not built

- **No silent or hidden mode.** No setting that suppresses the reminder while
  continuing to record.
- **No disguised process name.** It appears as what it is.
- **No cloud upload** (R16). Recordings stay on your disk, which is also the
  simplest privacy story available.

## Not legal advice

Obligations described here were researched in Sept 2026. If a recording will be
used for anything beyond personal reference, get advice for your jurisdiction.
