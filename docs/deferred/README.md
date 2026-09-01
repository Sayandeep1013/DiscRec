# Deferred

Work that was specced, researched, and then put down. Nothing here is being
built. It is kept because the research cost real effort and remains accurate.

| Doc | What it is | Why deferred |
|---|---|---|
| [capture-bot-dave.md](capture-bot-dave.md) | Recording via a Discord bot inside the call's encryption group | The only way to get **per-person tracks**. Dropped because it needs a hosted bot, a Discord application, and server admin rights — none of which fit a one-binary, press-record app |
| [bot-deployment.md](bot-deployment.md) | Where such a bot would run, secrets, restart behaviour | Same |
| [capture-linux.md](capture-linux.md) | PipeWire capture | Only Windows and macOS are targets |

## What is still worth reading here

The DAVE findings in [capture-bot-dave.md](capture-bot-dave.md) and
[../research/dave-protocol.md](../research/dave-protocol.md) establish why
network interception is impossible and how a bot would legitimately decrypt a
call. If per-person tracks ever become worth the setup burden, that is the
route, and `@snazzah/davey` is the library.

Nothing in the current build depends on any of it — DiscRec captures audio
*after* Discord decrypts it, so encryption is irrelevant to the shipped product.
