# Agent entry

| Machine | Read first |
|---|---|
| **macOS** | [docs/AGENT-macos-e2e.md](docs/AGENT-macos-e2e.md) — execute those jobs in order. Do not start from `HANDOFF.md`. |
| Windows | [docs/HANDOFF.md](docs/HANDOFF.md), then [CLAUDE.md](CLAUDE.md) |

`docs/HANDOFF.md` is the Windows GNU / `D:` environment. A Mac session that follows it will waste time on `dlltool` and skip the signed `.app` path that TCC requires.
