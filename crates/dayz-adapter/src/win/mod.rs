//! Everything that touches the running game.

mod data;
mod event_fields;
mod game_events;
mod gamemem;
mod guard;
mod memory;
mod service;
mod session;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use uniloader_sdk::api::PluginHandle;
use uniloader_sdk::Host;

pub(crate) use service::TABLE;

/// Whether something runs [`session::run_pending`] on the render thread every frame.
static TICKING: AtomicBool = AtomicBool::new(false);

/// Whether requests that must run on the game's thread can be served.
pub(crate) fn ticking() -> bool {
    TICKING.load(Ordering::Acquire)
}

/// The executable's file name, hashed to find its build in the database.
const EXECUTABLE: &str = "DayZ_x64.exe";

/// Where the address database is: `plugins/dayz-data/`, or the standalone loader's old
/// `plugin-loader/data/` so an existing install keeps working.
fn data_dir(host: &Host) -> PathBuf {
    let preferred = host.plugin_dir().join("dayz-data");
    let legacy = host.game_dir().join("plugin-loader").join("data");
    if !preferred.is_dir() && legacy.is_dir() {
        log::info!("dayz-data: using the old loader's {}", legacy.display());
        return legacy;
    }
    preferred
}

/// Resolve the database and hook the engine. Called once from `start`.
pub(crate) fn install(host: Host) {
    let exe = host
        .exe_path()
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(EXECUTABLE));
    if !exe {
        log::warn!(
            "{} is not {EXECUTABLE}; the address database will most likely not match",
            host.exe_path().display()
        );
    }
    data::initialize(&data_dir(&host), &host.exe_path());
    if let Some(resolved) = data::resolved() {
        event_fields::initialize(&resolved.table);
        game_events::install(host, &resolved.table, resolved.module_base as usize);
    }
}

/// Start draining session requests from the overlay's present callback.
pub(crate) fn tick_from(overlay: &uniloader_overlay::Overlay) {
    match overlay.on_present(|_| session::run_pending()) {
        Ok(()) => TICKING.store(true, Ordering::Release),
        Err(e) => log::warn!("no render-thread tick from the overlay: {e}"),
    }
}

/// The overlay went away, and with it the tick.
pub(crate) fn stop_ticking() {
    TICKING.store(false, Ordering::Release);
}

/// A plugin stopped or faulted: never call it again.
pub(crate) fn forget(plugin: PluginHandle) {
    game_events::forget(plugin);
}

/// `dayz.symbols [prefix]`.
pub(crate) fn symbols(prefix: Option<&str>) -> Vec<String> {
    let lines = data::console_lines(prefix);
    if lines.is_empty() {
        vec!["no symbols resolved; see the log for the dayz-data lines".to_owned()]
    } else {
        lines
    }
}

/// `dayz.game`.
pub(crate) fn game() -> Vec<String> {
    game_events::summary()
}

/// `dayz.read <target> [bytes]`.
pub(crate) fn read(target: &str, count: Option<usize>) -> Vec<String> {
    memory::console_lines(target, count)
}

/// `dayz.connect`.
pub(crate) fn connect(host: &str, port: u16) -> String {
    session::request_connect(host, port)
}

/// `dayz.disconnect`.
pub(crate) fn disconnect() -> String {
    session::request_disconnect()
}

/// `dayz.say`.
pub(crate) fn say(text: &str) -> Result<(), String> {
    session::request_chat(text, "")
}
