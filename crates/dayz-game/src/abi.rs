//! The `dayz` service table and the structures it hands over.
//!
//! Everything is `#[repr(C)]` and follows uniloader's ABI conventions: strings are borrowed
//! [`Str`]s valid for the call, growing structures start with `struct_size`, and enum-like
//! values are transparent integers so a newer adapter cannot hand an older plugin a value it
//! has undefined behaviour for.
//!
//! The adapter hooks three places in the engine:
//!
//! - [`GameEvent`] is every event the engine's broadcaster carries, named by its own class,
//!   from `ConnectingStartEvent` through `MPConnectionCloseEvent` to `RespawnEvent`.
//! - [`ChatMessage`] is caught one level lower, where the engine builds its chat event out of
//!   a channel and three strings, so the fields arrive already separated, and where returning
//!   [`GameResponse::SWALLOW`] keeps the line off the screen.
//! - [`RemoteCall`] is a mod's own RPC, on its way to script's `OnRPC`.

use core::ffi::c_void;

use uniloader_sdk::api::{PluginHandle, Status, Str};

/// How one of an event's fields is stored, and what [`GameField::address`] points at.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldKind(pub u32);

impl FieldKind {
    /// A kind this ABI version has no name for. Only [`GameField::text`] is meaningful.
    pub const UNKNOWN: FieldKind = FieldKind(0);
    /// Four-byte signed integer.
    pub const INT: FieldKind = FieldKind(1);
    /// Four-byte float.
    pub const FLOAT: FieldKind = FieldKind(2);
    /// One byte holding 0 or 1.
    pub const BOOL: FieldKind = FieldKind(3);
    /// Pointer to an engine string holder. Already decoded into [`GameField::text`].
    pub const STRING: FieldKind = FieldKind(4);
    /// Three consecutive floats.
    pub const VECTOR: FieldKind = FieldKind(5);
    /// Pointer to some other object, reported but not followed.
    pub const OBJECT: FieldKind = FieldKind(6);
    /// Bare `char *`. Also already decoded into [`GameField::text`]; the engine uses both
    /// this and [`FieldKind::STRING`] and they are not interchangeable.
    pub const CSTRING: FieldKind = FieldKind(7);
}

/// One field of one event, read out of the event object and rendered.
///
/// `text` is always set and is all a plugin that logs events needs. `kind` and `address` are
/// for a plugin that wants the value itself: `address` is the field's location inside the
/// event, because that is the one answer with the same meaning for every kind.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GameField {
    /// `size_of::<GameField>()`.
    pub struct_size: usize,
    /// The field's name, as the engine's own script parameters name it.
    pub name: Str,
    /// How it is stored.
    pub kind: FieldKind,
    /// The value, formatted. Valid for the duration of the call.
    pub text: Str,
    /// Where the field is, inside the event object. Readable for the duration of the call.
    pub address: u64,
}

/// One event the engine raised.
///
/// `name` is the event's own class name, read out of the event, so it is right for classes
/// nobody has described. `event` and everything `fields` points at are valid for the call.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GameEvent {
    /// `size_of::<GameEvent>()`.
    pub struct_size: usize,
    /// Class name, for example `ChatMessageEvent` or `MPConnectionCloseEvent`.
    pub name: Str,
    /// The `enf::Event` object.
    pub event: *mut c_void,
    /// The decoded fields, in the order the engine's own script parameters list them. Null
    /// with a count of zero for a class the address database has no layout for.
    pub fields: *const GameField,
    /// How many [`GameEvent::fields`] there are.
    pub field_count: usize,
}

/// One event class the adapter knows the shape of, whether or not it has been raised.
///
/// The strings belong to the adapter and last as long as the process, so they may be kept.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GameClass {
    /// `size_of::<GameClass>()`.
    pub struct_size: usize,
    /// The class name, which is what [`GameEvent::name`] will be.
    pub name: Str,
    /// What the event means, or empty.
    pub note: Str,
    /// How many fields the adapter can decode for it. Zero for a class known by name only.
    pub field_count: usize,
}

/// One chat line, as the engine was about to show it. Empty strings are `""`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ChatMessage {
    /// `size_of::<ChatMessage>()`.
    pub struct_size: usize,
    /// Channel the line is on, as the engine numbers them.
    pub channel: u32,
    /// Who said it. Empty for a line the game itself produced.
    pub from: Str,
    /// What was said.
    pub text: Str,
    /// Name of the colour class the game would draw it in.
    pub colour: Str,
}

/// One remote call on its way to script, passed through uninterpreted.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RemoteCall {
    /// `size_of::<RemoteCall>()`.
    pub struct_size: usize,
    /// Identity the call came from, script's `PlayerIdentity`. Null when the server sent it.
    pub sender: *mut c_void,
    /// Object the call is addressed to, script's `Object`. Null for a global call.
    pub target: *mut c_void,
    /// The mod's own call number, script's `rpc_type`.
    pub kind: u32,
    /// Serialised parameters, script's `ParamsReadContext`.
    pub params: *mut c_void,
}

/// Whether the game should still see a chat line a plugin was just shown.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameResponse(pub u32);

impl GameResponse {
    /// Let it through. The default, and what an unknown value is read as.
    pub const PASS: GameResponse = GameResponse(0);
    /// Swallow it: the engine never draws it. The first subscriber to swallow ends delivery.
    pub const SWALLOW: GameResponse = GameResponse(1);
}

/// Bit flags for [`DayzApi::listen`], one per stream.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameMask(pub u32);

impl GameMask {
    /// Subscribe to nothing, which is how a plugin unsubscribes.
    pub const NONE: GameMask = GameMask(0);
    /// [`GameEvent`], every event the broadcaster carries.
    pub const EVENTS: GameMask = GameMask(1);
    /// [`ChatMessage`], with the chance to swallow.
    pub const CHAT: GameMask = GameMask(2);
    /// [`RemoteCall`], a mod's RPC on its way to script.
    pub const RPC: GameMask = GameMask(4);
    /// Every stream this version knows.
    pub const ALL: GameMask = GameMask(7);

    /// Whether this mask includes that stream.
    #[must_use]
    pub const fn covers(self, other: GameMask) -> bool {
        self.0 & other.0 != 0
    }
}

impl core::ops::BitOr for GameMask {
    type Output = GameMask;

    fn bitor(self, other: GameMask) -> GameMask {
        GameMask(self.0 | other.0)
    }
}

/// `on_event(ctx, event)`.
pub type EventFn = unsafe extern "C" fn(ctx: *mut c_void, event: *const GameEvent);
/// `on_chat(ctx, message) -> response`.
pub type ChatFn =
    unsafe extern "C" fn(ctx: *mut c_void, message: *const ChatMessage) -> GameResponse;
/// `on_rpc(ctx, call)`.
pub type RpcFn = unsafe extern "C" fn(ctx: *mut c_void, call: *const RemoteCall);

/// Where one subscriber wants the streams delivered. Copied by the adapter.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GameSink {
    /// `size_of::<GameSink>()`.
    pub struct_size: usize,
    /// Handed back to every callback.
    pub ctx: *mut c_void,
    /// Required for [`GameMask::EVENTS`].
    pub on_event: Option<EventFn>,
    /// Required for [`GameMask::CHAT`].
    pub on_chat: Option<ChatFn>,
    /// Required for [`GameMask::RPC`].
    pub on_rpc: Option<RpcFn>,
}

/// The `dayz` service, version 1.
///
/// Every function may be called from any thread. Callbacks arrive on the game's own thread,
/// under the loader's fault guard for the subscriber.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DayzApi {
    /// `size_of::<DayzApi>()` as the adapter was compiled.
    pub struct_size: usize,
    /// The game build the database matched, for example `1.29.163709`; empty when unknown.
    /// The string lives as long as the process.
    pub build: unsafe extern "C" fn() -> Str,
    /// Absolute address of a named symbol, for example `render.frame`.
    pub symbol: unsafe extern "C" fn(name: Str, out: *mut u64) -> Status,
    /// A named structure offset, for example `framebase.rotation`.
    pub offset: unsafe extern "C" fn(name: Str, out: *mut u64) -> Status,
    /// Subscribe `owner` to the streams in `mask` (the last call wins; `NONE` unsubscribes).
    /// `UNSUPPORTED` when this build's hooks are not in, `INVALID_ARGUMENT` when `sink` lacks
    /// a callback for a requested stream.
    pub listen:
        unsafe extern "C" fn(owner: PluginHandle, mask: GameMask, sink: *const GameSink) -> Status,
    /// Fill `out` with up to `capacity` known event classes, sorted by name, and set `total`
    /// to how many there are. `BUFFER_TOO_SMALL` when `total > capacity`.
    pub catalogue:
        unsafe extern "C" fn(out: *mut GameClass, capacity: usize, total: *mut usize) -> Status,
    /// Put one line in this client's own chat, on the next frame. `colour` names a colour
    /// class such as `ColorFriendly`; empty takes the game's default. `NOT_FOUND` until the
    /// game object is known, which is once a session has started.
    pub chat_local: unsafe extern "C" fn(text: Str, colour: Str) -> Status,
    /// The engine's `CGame` instance, or null before a session has started.
    pub game_instance: unsafe extern "C" fn() -> *mut c_void,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_covers_every_named_stream() {
        for one in [GameMask::EVENTS, GameMask::CHAT, GameMask::RPC] {
            assert!(GameMask::ALL.covers(one), "{one:?}");
            assert!(!GameMask::NONE.covers(one), "{one:?}");
        }
    }

    #[test]
    fn a_mask_covers_what_was_put_in_it() {
        let mask = GameMask::CHAT | GameMask::RPC;
        assert!(mask.covers(GameMask::CHAT));
        assert!(mask.covers(GameMask::RPC));
        assert!(!mask.covers(GameMask::EVENTS));
    }
}
