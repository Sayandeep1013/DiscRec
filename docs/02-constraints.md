# Platform constraints

What the operating systems and Discord actually permit. Each claim links to the
research note holding its source and check date.

## C1 — Both target OSes support per-process audio capture

This is the constraint the product depends on, and it is recent. Before ~2021
this needed a virtual audio driver or a kernel extension.

| OS | API | Minimum |
|---|---|---|
| Windows | WASAPI process loopback (`ActivateAudioInterfaceAsync` with `VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK`) | Win10 build 20348 |
| macOS | Core Audio process taps (`AudioHardwareCreateProcessTap`) | macOS 14.2 (14.4+ preferred) |

**Consequence:** capturing Discord alone is possible without a driver, which is
what makes a single self-contained binary feasible (R6).

→ [research/platform-audio-apis.md](research/platform-audio-apis.md)

## C2 — Capture can succeed and produce silence

On both platforms the capture API can return success while delivering nothing:
the wrong process ID, a process tree not included, or a permission granted at
the dialog but not in effect.

**Consequence:** never trust a success code. Assert real signal within the first
seconds and treat its absence as an error ([P2](05-challenges.md#p2)).

## C3 — The two capture streams have independent clocks

Discord's loopback endpoint and the microphone are different devices with
different hardware oscillators. They run at *almost* the same rate. Over an
hour, "almost" is audible desynchronisation.

**Consequence:** drift compensation is mandatory before mixing, and because the
mix is written at capture time the error is permanent
([P1](05-challenges.md#p1)).

## C4 — macOS gates audio capture behind TCC, and Gatekeeper behind notarization

Process taps require a user permission grant. An unsigned binary requesting
audio capture is treated as malware-adjacent and will not run for a normal user.

**Consequence:** an Apple Developer account and a notarization step are required
to ship on macOS. Handle permission denial as a first-class state, not an error
path ([P3](05-challenges.md#p3)).

## C5 — Mobile cannot do this at all

Android's `AudioPlaybackCapture` admits only `USAGE_MEDIA`, `USAGE_GAME` and
`USAGE_UNKNOWN`; Discord voice is `USAGE_VOICE_COMMUNICATION`, excluded by
design. iOS has no system-audio capture API.

**Consequence:** out of scope permanently, not pending
([ADR-0006](adr/0006-mobile-out-of-scope.md)). Recorded here because it is the
first thing anyone proposes.

## C6 — Discord's Terms require telling participants

Independent of local law, and enforced by account suspension. The app has no
channel to announce this on — it does not talk to Discord at all.

**Consequence:** the obligation sits entirely with the user, and the product's
only honest response is to make it hard to forget
([06-legal-and-consent.md](06-legal-and-consent.md)).

## Not a constraint: Discord's encryption

Discord end-to-end encrypts all voice (DAVE, enforced March 2026). This is
**irrelevant to DiscRec**, which captures audio after Discord has decrypted and
rendered it. It matters only to bot-based recorders.

Noted because it is a natural question, and because it permanently rules out any
design based on intercepting network traffic.
→ [research/dave-protocol.md](research/dave-protocol.md)
