//! The `dayz` service: what the DayZ adapter plugin offers other plugins, and a safe client.
//!
//! uniloader knows nothing about DayZ. The adapter plugin (`dayz.dll`) does: it resolves the
//! [dayz-data](https://github.com/dayz-plugins/dayz-data) address database against the
//! running game, hooks the engine's event broadcaster, chat and remote calls, and offers all
//! of it as the service `dayz`. A feature plugin declares a dependency on the plugin
//! [`PLUGIN`] and calls [`Dayz::connect`]:
//!
//! ```ignore
//! const DEPENDENCIES: &'static [Dependency] = &[Dependency::plugin(dayz_game::PLUGIN, ">=0.1")];
//!
//! fn start(host: Host) -> Result<Self, PluginError> {
//!     let dayz = Dayz::connect(&host).ok_or("the dayz adapter is not running")?;
//!     dayz.require(&["render.frame"])?;
//!     let frame = dayz.symbol("render.frame")?;
//!     dayz.listen(Streams::EVENTS, Arc::clone(&state))?;
//!     ...
//! }
//! ```
//!
//! ## Events
//!
//! [`Event::name`] is the event's own class name, read out of the engine, so it is right for
//! classes nobody has described. [`Event::fields`] is its contents, already read and
//! rendered, for the classes whose layout is in the address database; an event with no fields
//! means nobody has established that class's layout yet, and [`Event::object`] is still there
//! for a plugin that knows better. [`Dayz::catalogue`] is the same knowledge up front.

mod abi;
mod client;

pub use abi::{
    ChatFn, ChatMessage, DayzApi, EventFn, FieldKind, GameClass, GameEvent, GameField, GameMask,
    GameResponse, GameSink, RemoteCall, RpcFn,
};
pub use client::{Chat, Class, Dayz, Event, Field, GameListener, Rpc};

/// The streams a plugin can ask for.
pub use abi::GameMask as Streams;
/// What the game should do with a chat line a plugin has just seen.
pub use abi::GameResponse as ChatVerdict;

/// Name the adapter registers its service under.
pub const SERVICE: &str = "dayz";
/// Version of [`DayzApi`] this crate describes.
pub const VERSION: u32 = 1;
/// Name of the plugin that provides the service, for a dependency declaration.
pub const PLUGIN: &str = "dayz";
