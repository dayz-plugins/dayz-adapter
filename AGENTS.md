# Agent notes: dayz-adapter

## What this repository is

The DayZ adapter plugin for uniloader (`crates/dayz-adapter`, DLL `dayz.dll`) and the client
crate other plugins use to reach it (`crates/dayz-game`). It replaced the game-specific half of
the standalone [dayz-plugin-loader](https://github.com/dayz-plugins/dayz-plugin-loader): the
address database, the event/chat/RPC hooks and the session natives moved here; the generic half
became uniloader.

Prose documentation belongs in
[dayz-plugins.github.io](https://github.com/dayz-plugins/dayz-plugins.github.io); this
repository carries `README.md` and `AGENTS.md` only.

## Rules that matter

- **Addresses are data, never constants.** They resolve by name from dayz-data. Never compile
  an address into this crate or let a plugin do it.
- **The service ABI is append-only.** `DayzApi` and every struct with `struct_size` only ever
  gain fields at the end; bump `dayz_game::VERSION` only when a field changes meaning.
- **Hooks go in during `start`**, never on subscription: patching a function another thread
  may be executing is the one risk a detour cannot remove, and `start` runs before a session
  exists. Subscribing only flips a mask.
- **Every delivery goes through `Host::guarded`** for the subscriber, so a faulting listener is
  disabled on its own and the game carries on.
- **Engine calls run on the render thread.** `connect`, `disconnect` and local chat are queued
  and drained from the overlay's present callback; calling them from the console thread
  deadlocked a live session once.
- **`dayz.read` uses its own structured handler** (`win/guard.rs`): a bad address typed into the
  console is not the adapter's fault and must not disable it.

## Checks

```bash
scripts/gate.sh          # fmt, clippy (pedantic, -D warnings), tests, docs, Windows clippy, build
scripts/gate.sh --host   # host half only
```

The game-facing modules (`win/`) only build for Windows; the address parser and the client
crate test on the host. In-game verification: `scripts/deploy.sh`, `scripts/run-dayz.sh`, then
`dayz-ctl exec dayz.game` from the debug plugin.

Commits: imperative subject, one revertable change each, trailer
`Co-Authored-By: <model> <noreply@anthropic.com>`. GitHub Actions never run under this account:
any workflow keeps only `workflow_dispatch` live.
