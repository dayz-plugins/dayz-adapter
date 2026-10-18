# dayz-adapter

The DayZ adapter for [uniloader](https://uniloaders.github.io/): one plugin, `dayz.dll`, that
holds everything other plugins need to know about DayZ, so none of them has to.

- **Addresses by name.** It resolves the [dayz-data](https://github.com/dayz-plugins/dayz-data)
  database against the running executable (hash first, byte patterns as the fallback) and
  answers `render.execute_view` or `framebase.rotation` with an address or an offset for this
  build. No plugin compiles an address in.
- **The game's own streams.** It hooks the engine's event broadcaster, its chat path and its
  remote-call dispatcher once, at start, and hands every event, chat line and RPC to the
  plugins that listen. Event names are read from the events themselves; fields are decoded for
  the classes the database has layouts for.
- **Session control.** `connect`, `disconnect` and a line in your own chat, run on the game's
  render thread through the overlay plugin's present callback.

It is offered as the service `dayz`; plugins in Rust use the `dayz-game` client crate in this
repository.

## Installing

Put into the game folder (`steamapps/common/DayZ`):

```text
dxgi.dll                    uniloader.dll, renamed
plugins/
  overlay.dll               uniloader's overlay (needed for connect/disconnect/chat, toasts)
  dayz.dll                  this adapter
  dayz-data/                patterns.json and builds/*.json from dayz-data
  gamelog.dll, patches.dll, debug.dll, ...   the feature plugins
```

On Linux, launch with `WINEDLLOVERRIDES="dxgi=n,b" %command% -nobe`. DayZ's BattlEye refuses
modified clients on protected servers; this is for offline play, community servers without
BattlEye, and development.

`scripts/deploy.sh` installs uniloader, the overlay, the adapter and the database from sibling
checkouts; `scripts/run-dayz.sh` starts the game through Proton without the launcher.

## Console

| Command | Does |
| --- | --- |
| `dayz.symbols [prefix]` | The matched build, and every resolved address and offset. |
| `dayz.game` | Which streams are hooked, how much has passed, who listens. |
| `dayz.read [*]<symbol\|0xaddr>[+hex] [bytes]` | Dump game memory; `*` follows the pointer first. Faults are caught. |
| `dayz.connect <host>[:port]` | Join a server (port 2302 by default). |
| `dayz.disconnect` | Leave the server. |
| `dayz.say <text>` | A line in your own chat; nobody else sees it. |

`connect`, `disconnect` and `say` need a session to have started once (the engine hands the
adapter its game object on the first chat line or remote call) and the overlay plugin.

## Writing a DayZ plugin

```rust
use std::sync::Arc;
use dayz_game::{Dayz, Event, GameListener, Streams};
use uniloader_sdk::{export_plugin, Dependency, Host, Plugin, PluginError};

struct Watcher;

impl GameListener for Watcher {
    fn on_game_event(&self, event: &Event<'_>) {
        if event.is("RespawnEvent") {
            log::info!("respawned");
        }
    }
}

struct MyPlugin;

impl Plugin for MyPlugin {
    const NAME: &'static str = "watcher";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const DESCRIPTION: &'static str = "Logs respawns.";
    const DEPENDENCIES: &'static [Dependency] = &[Dependency::plugin(dayz_game::PLUGIN, ">=0.1")];

    fn start(host: Host) -> Result<Self, PluginError> {
        let dayz = Dayz::connect(&host).ok_or_else(|| "no dayz adapter".to_owned())?;
        dayz.require(&["render.frame"])?;       // refuse to start on a build without it
        let _frame = dayz.symbol("render.frame")?;
        dayz.listen(Streams::EVENTS, Arc::new(Watcher))?;
        Ok(MyPlugin)
    }
}

export_plugin!(MyPlugin);
```

[`dayz-game`](crates/dayz-game/src/abi.rs) documents the C layout for other languages.

## Building

Needs sibling checkouts of [uniloader](https://github.com/uniloaders/uniloader)
(`../../uniloader`) and [dayz-plugin-loader](https://github.com/dayz-plugins/dayz-plugin-loader)
(for the `dayz-data` reader crate), and the `build-box` container:

```bash
scripts/gate.sh          # fmt, clippy, tests, docs, Windows clippy, release build
scripts/deploy.sh        # install into the game
scripts/run-dayz.sh      # start it
```

## Related repositories

| Repository | What it is |
| --- | --- |
| [uniloader](https://github.com/uniloaders/uniloader) | The loader, the SDK, the overlay |
| [dayz-data](https://github.com/dayz-plugins/dayz-data) | Addresses and offsets per game build |
| [dayz-log-plugin](https://github.com/dayz-plugins/dayz-log-plugin) | Logs events, chat and RPCs |
| [dayz-patches-plugin](https://github.com/dayz-plugins/dayz-patches-plugin) | Engine bug workarounds |
| [dayz-debug-plugin](https://github.com/dayz-plugins/dayz-debug-plugin) | The console on a socket, and `dayz-ctl` |

## License

[Unlicense](LICENSE).
