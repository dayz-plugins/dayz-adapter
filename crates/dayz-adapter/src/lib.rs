//! The DayZ adapter: what uniloader plugins need to know about DayZ, in one plugin.
//!
//! uniloader is game-agnostic. This plugin (`dayz.dll`, plugin name `dayz`) is where DayZ
//! knowledge lives:
//!
//! - It resolves the [dayz-data](https://github.com/dayz-plugins/dayz-data) address database
//!   against the running executable, so other plugins ask for `render.frame` by name instead
//!   of compiling an address in.
//! - It hooks the engine's event broadcaster, chat and remote-call paths once, at start, and
//!   hands those streams to every plugin that listens.
//! - It offers both as the service `dayz` ([`dayz_game::DayzApi`]); feature plugins use the
//!   `dayz-game` client crate.
//! - Console commands: `dayz.symbols`, `dayz.game`, `dayz.read`, `dayz.connect`,
//!   `dayz.disconnect`, `dayz.say`.
//!
//! Calls into the game's own functions (`connect`, `disconnect`, local chat) have to run on
//! its render thread. The adapter gets there through the overlay plugin's present callback,
//! so those three need `overlay.dll`; everything else works without it.

// Only Windows has a game to adapt to; elsewhere the plugin builds for the unit tests alone.
#![cfg_attr(
    not(windows),
    allow(dead_code, reason = "the game side is Windows only")
)]

mod address;
#[cfg(windows)]
mod win;

use uniloader_overlay::Overlay;
use uniloader_sdk::{
    export_plugin, topics, Dependency, Host, Plugin, PluginError, PluginRef, PluginState,
    PluginStateEvent,
};

/// The plugin instance. The hooks and the service live in statics, because the game's threads
/// reach them without a `&self`.
pub struct Adapter {
    /// The overlay's handle, to notice it stopping and with it the render-thread tick.
    overlay: Option<PluginRef>,
}

/// The value of a command argument that must be there.
fn required<'a>(args: &'a str, what: &str) -> Result<&'a str, PluginError> {
    let args = args.trim();
    if args.is_empty() {
        Err(PluginError::from(format!("missing {what}")))
    } else {
        Ok(args)
    }
}

impl Plugin for Adapter {
    const NAME: &'static str = "dayz";
    const TITLE: &'static str = "DayZ";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUTHORS: &'static str = "Bluscream";
    const DESCRIPTION: &'static str =
        "DayZ adapter: the address database and the game's events, chat and remote calls.";
    const DEPENDENCIES: &'static [Dependency] =
        &[Dependency::plugin(uniloader_overlay::PLUGIN, ">=0.1").optional()];

    // Providing the service is unsafe: the table layout is a promise the compiler cannot check.
    #[allow(unsafe_code)]
    fn start(host: Host) -> Result<Self, PluginError> {
        host.command(
            "symbols",
            "List resolved game addresses and offsets.",
            "[prefix]",
        )?;
        host.command(
            "game",
            "Show which of the game's streams are hooked and watched.",
            "",
        )?;
        host.command(
            "read",
            "Dump game memory; * reads the pointer there first.",
            "[*]<symbol|0xaddr>[+hexoffset] [bytes]",
        )?;
        host.command(
            "connect",
            "Join a server; the port defaults to 2302.",
            "<host>[:port]",
        )?;
        host.command(
            "disconnect",
            "Leave the server and go back to the menu.",
            "",
        )?;
        host.command(
            "say",
            "Put a line in your own chat; nobody else sees it.",
            "<text>",
        )?;
        host.subscribe(topics::PLUGIN_STATE)?;
        #[cfg(windows)]
        {
            win::install(host);
            // SAFETY: `win::TABLE` is the `DayzApi` layout of this version, and every function
            // in it is thread safe.
            unsafe { host.provide(dayz_game::SERVICE, dayz_game::VERSION, &win::TABLE)? };
        }
        let overlay = Overlay::connect(&host);
        #[cfg(windows)]
        if let Some(overlay) = &overlay {
            win::tick_from(overlay);
        }
        if overlay.is_none() {
            log::info!("no overlay: dayz.connect, dayz.disconnect and local chat are unavailable");
        }
        Ok(Adapter {
            overlay: overlay.and_then(|_| host.find_plugin(uniloader_overlay::PLUGIN)),
        })
    }

    fn on_command(&self, host: &Host, name: &str, args: &str) -> Result<(), PluginError> {
        let lines = commands(name, args)?;
        for line in lines {
            host.console_print(&line);
        }
        Ok(())
    }

    fn on_event(&self, _host: &Host, _from: PluginRef, topic: &str, payload: &[u8]) {
        if topic != topics::PLUGIN_STATE {
            return;
        }
        let Some(event) = PluginStateEvent::parse(payload) else {
            return;
        };
        if event.state == PluginState::STOPPED || event.state == PluginState::FAULTED {
            self.ended(event.plugin);
        }
    }
}

impl Adapter {
    /// A plugin stopped or faulted: never call it again, and if it was the overlay, the
    /// render-thread tick went with it.
    fn ended(&self, plugin: PluginRef) {
        #[cfg(windows)]
        {
            win::forget(plugin.raw());
            if Some(plugin) == self.overlay {
                win::stop_ticking();
            }
        }
        #[cfg(not(windows))]
        let _ = (plugin, self.overlay);
    }
}

/// Run one of the adapter's commands, returning what to print.
#[cfg(windows)]
fn commands(name: &str, args: &str) -> Result<Vec<String>, PluginError> {
    match name {
        "symbols" => Ok(win::symbols(Some(args.trim()).filter(|p| !p.is_empty()))),
        "game" => Ok(win::game()),
        "read" => {
            let mut words = required(args, "a symbol or address")?.split_whitespace();
            let target = words.next().unwrap_or_default();
            let count = match words.next() {
                Some(n) => Some(
                    n.parse::<usize>()
                        .map_err(|_| PluginError::from(format!("{n:?} is not a byte count")))?,
                ),
                None => None,
            };
            Ok(win::read(target, count))
        }
        "connect" => {
            let (host, port) = address::parse(args)?;
            Ok(vec![win::connect(&host, port)])
        }
        "disconnect" => Ok(vec![win::disconnect()]),
        "say" => {
            win::say(required(args, "the text")?)?;
            Ok(Vec::new())
        }
        _ => Err(PluginError::Unsupported),
    }
}

/// Without the game, every command says so.
#[cfg(not(windows))]
fn commands(_name: &str, _args: &str) -> Result<Vec<String>, PluginError> {
    Err(PluginError::from("DayZ runs on Windows only".to_owned()))
}

export_plugin!(Adapter);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_argument_is_named() {
        assert!(required("  ", "the text").is_err());
        assert_eq!(required(" hi ", "the text").ok(), Some("hi"));
    }
}
