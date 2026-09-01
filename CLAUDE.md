# Working notes for DiscRec

**Starting a fresh session? Read `docs/PROJECT-LOG.md` first.** It carries the
context that is not recoverable from code or git history: why the scope changed
three times, what was researched and rejected, and which conclusions were
reversed.

**What this is:** a small Windows/macOS app that captures Discord's audio when
you press record. One binary, no background service, no bot, no configuration
to speak of.

## Settled — do not relitigate

- **Manual control only.** The user opens the app and presses record. There is
  no auto-start, no join detection, no Discord RPC, no background watcher. This
  removed the most fragile subsystem in the design; do not reintroduce it.
  → [ADR-0008](docs/adr/0008-manual-control.md)
- **No bot.** Per-person tracks require being inside the call and are not worth
  a hosted bot plus a Discord application. The research is preserved in
  `docs/deferred/` and is not a backlog item.
- **Encryption is irrelevant.** DiscRec captures audio *after* Discord decrypts
  it. DAVE, E2EE and the voice protocol do not affect this product at all.
- **Mobile is impossible**, not merely out of scope
  ([ADR-0006](docs/adr/0006-mobile-out-of-scope.md)).
- **No Electron.** Footprint is the product's reason to exist.

## The two defects that matter

Both are silent — they produce files that exist and play, and are wrong.

1. **Clock drift between capture streams.** Discord's loopback and the
   microphone are separate devices with separate hardware clocks. Un-compensated,
   your voice slides out of sync with everyone else's, and because the mix is
   written at capture time it cannot be fixed afterwards.
   → `docs/spec/mixing-and-timeline.md`
2. **Silent capture.** Both platforms can return a healthy stream that carries
   nothing — wrong PID, or a denied permission. Always assert real signal early
   rather than trusting a success code.

## Conventions

- Specs in `docs/spec/`, decisions in `docs/adr/`, platform findings with
  sources in `docs/research/`, abandoned work in `docs/deferred/`.
- Requirements are `R1..Rn` in `docs/04-requirements.md`; challenges `P1..Pn` in
  `docs/05-challenges.md`. Cite the ID when a spec addresses one.
- Platform-specific code lives only in `src/capture/{windows,macos}.rs`. If a
  `#[cfg(...)]` is needed anywhere else, that is a design smell worth raising.

## Verify before trusting

Library and API facts were researched in Sept 2026 and age fast. Re-check any
claim about `cpal`, `flexaudio`, or the platform capture APIs against what is
actually published before depending on it. → `docs/research/`

## Commits

Do not add co-author or attribution trailers to commit messages.
