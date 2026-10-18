//! A safe wrapper over [`DayzApi`] for Rust plugins.
//!
//! A listener handed to the adapter is boxed and leaked: the adapter keeps the pointer for as
//! long as it might call it, and a plugin's DLL is never unloaded. A panic inside one is caught
//! at the boundary and logged; it never unwinds into the game.

// The consumer side of the service ABI; every unsafe block names the contract it relies on.
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};

use uniloader_sdk::api::{PluginHandle, Status, Str};
use uniloader_sdk::{Host, PluginError};

use crate::abi::{
    ChatMessage, DayzApi, FieldKind, GameClass, GameEvent, GameMask, GameResponse, GameSink,
    RemoteCall,
};

/// Room made for the catalogue before asking for it. This build has 77 classes; being wrong
/// costs a second call rather than a wrong answer.
const CATALOGUE_GUESS: usize = 128;

fn check(status: Status) -> Result<(), PluginError> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(PluginError::Status(status))
    }
}

/// Borrow a string the adapter handed over.
///
/// # Safety
/// `s` must describe readable bytes for `'a`.
unsafe fn borrow<'a>(s: Str) -> &'a str {
    if s.ptr.is_null() || s.len == 0 {
        return "";
    }
    // SAFETY: the caller guarantees `len` readable bytes at `ptr` for `'a`.
    let bytes = unsafe { core::slice::from_raw_parts(s.ptr, s.len) };
    core::str::from_utf8(bytes).unwrap_or("")
}

/// One field of one event, read out of the event and rendered.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Field<'a> {
    /// The field's name, as the engine's own script parameters name it.
    pub name: &'a str,
    /// The value, formatted. This is the point of a field and is always set.
    pub text: &'a str,
    /// How it is stored, for a plugin that wants the value rather than a rendering of it.
    pub kind: FieldKind,
    /// Where the field is, inside the event object. Readable for the length of the callback.
    pub address: u64,
}

/// One event class the adapter knows the shape of. The strings last as long as the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Class {
    /// The class name, which is what [`Event::name`] will be.
    pub name: &'static str,
    /// What the event means, or empty.
    pub note: &'static str,
    /// How many fields the adapter can decode for it. Zero for a class known by name only.
    pub field_count: usize,
}

/// One event the engine raised.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Event<'a> {
    /// The event's class name, for example `RespawnEvent`.
    pub name: &'a str,
    /// Its contents; empty for a class whose layout nobody has established yet.
    pub fields: &'a [Field<'a>],
    /// The event object itself. Valid only for the length of the callback.
    pub object: *mut c_void,
}

impl Event<'_> {
    /// Whether this is that class, by name.
    #[must_use]
    pub fn is(&self, class: &str) -> bool {
        self.name == class
    }
}

/// One chat line, before the game draws it.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Chat<'a> {
    /// Channel the line is on, as the engine numbers them.
    pub channel: u32,
    /// Who said it, empty for a line the game itself produced.
    pub from: &'a str,
    /// What was said.
    pub text: &'a str,
    /// Name of the colour class the game would draw it in.
    pub colour: &'a str,
}

/// One remote call on its way to script. `params` is the engine's own serialised stream.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Rpc {
    /// The mod's own call number, script's `rpc_type`.
    pub kind: u32,
    /// Identity the call came from, script's `PlayerIdentity`. Null when the server sent it.
    pub sender: *mut c_void,
    /// Object the call is addressed to, script's `Object`. Null for a global call.
    pub target: *mut c_void,
    /// Serialised parameters, script's `ParamsReadContext`.
    pub params: *mut c_void,
}

/// What a plugin implements to hear the game. Every method has a default that ignores it.
///
/// Called on the game's own thread, under the loader's fault guard for the listening plugin.
pub trait GameListener: Send + Sync + 'static {
    /// An event the engine raised. Needs [`GameMask::EVENTS`].
    fn on_game_event(&self, _event: &Event<'_>) {}
    /// A chat line before it is drawn. Needs [`GameMask::CHAT`].
    fn on_chat(&self, _chat: &Chat<'_>) -> GameResponse {
        GameResponse::PASS
    }
    /// A remote call on its way to script. Needs [`GameMask::RPC`].
    fn on_rpc(&self, _rpc: &Rpc) {}
}

impl<T: GameListener> GameListener for std::sync::Arc<T> {
    fn on_game_event(&self, event: &Event<'_>) {
        (**self).on_game_event(event);
    }
    fn on_chat(&self, chat: &Chat<'_>) -> GameResponse {
        (**self).on_chat(chat)
    }
    fn on_rpc(&self, rpc: &Rpc) {
        (**self).on_rpc(rpc);
    }
}

/// Run a listener callback, keeping a panic on this side of the ABI.
fn contained<R>(what: &str, fallback: R, f: impl FnOnce() -> R) -> R {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        log::error!("dayz {what} listener panicked");
        fallback
    })
}

unsafe extern "C" fn deliver_event<L: GameListener>(ctx: *mut c_void, raw: *const GameEvent) {
    if ctx.is_null() || raw.is_null() {
        return;
    }
    // SAFETY: `ctx` is the `L` leaked in `Dayz::listen` for this function; `raw` is valid for
    // the call, as are the fields it describes.
    let (listener, raw) = unsafe { (&*ctx.cast::<L>(), &*raw) };
    let fields: Vec<Field<'_>> = if raw.fields.is_null() || raw.field_count == 0 {
        Vec::new()
    } else {
        // SAFETY: the ABI describes `field_count` readable structures for the call.
        let slice = unsafe { core::slice::from_raw_parts(raw.fields, raw.field_count) };
        slice
            .iter()
            .map(|f| Field {
                // SAFETY: valid for the call.
                name: unsafe { borrow(f.name) },
                // SAFETY: valid for the call.
                text: unsafe { borrow(f.text) },
                kind: f.kind,
                address: f.address,
            })
            .collect()
    };
    let event = Event {
        // SAFETY: valid for the call.
        name: unsafe { borrow(raw.name) },
        fields: &fields,
        object: raw.event,
    };
    contained("event", (), || listener.on_game_event(&event));
}

unsafe extern "C" fn deliver_chat<L: GameListener>(
    ctx: *mut c_void,
    raw: *const ChatMessage,
) -> GameResponse {
    if ctx.is_null() || raw.is_null() {
        return GameResponse::PASS;
    }
    // SAFETY: as in `deliver_event`.
    let (listener, raw) = unsafe { (&*ctx.cast::<L>(), &*raw) };
    let chat = Chat {
        channel: raw.channel,
        // SAFETY: the three strings are valid for the call.
        from: unsafe { borrow(raw.from) },
        // SAFETY: as above.
        text: unsafe { borrow(raw.text) },
        // SAFETY: as above.
        colour: unsafe { borrow(raw.colour) },
    };
    contained("chat", GameResponse::PASS, || listener.on_chat(&chat))
}

unsafe extern "C" fn deliver_rpc<L: GameListener>(ctx: *mut c_void, raw: *const RemoteCall) {
    if ctx.is_null() || raw.is_null() {
        return;
    }
    // SAFETY: as in `deliver_event`.
    let (listener, raw) = unsafe { (&*ctx.cast::<L>(), &*raw) };
    let rpc = Rpc {
        kind: raw.kind,
        sender: raw.sender,
        target: raw.target,
        params: raw.params,
    };
    contained("rpc", (), || listener.on_rpc(&rpc));
}

/// The adapter, as one plugin sees it. Cheap to copy and usable from any thread.
#[derive(Clone, Copy)]
pub struct Dayz {
    api: &'static DayzApi,
    owner: PluginHandle,
}

impl core::fmt::Debug for Dayz {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Dayz")
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

impl Dayz {
    /// Find the adapter. `None` when no plugin provides the service, or one with an
    /// incompatible layout does. Declare a dependency on [`crate::PLUGIN`] so it is running.
    #[must_use]
    pub fn connect(host: &Host) -> Option<Dayz> {
        // SAFETY: `DayzApi` is the layout the service `dayz` promises at this version; the
        // version and size are checked before the table is used.
        let service = unsafe { host.service::<DayzApi>(crate::SERVICE) }?;
        if service.version != crate::VERSION
            || service.table.struct_size < core::mem::size_of::<DayzApi>()
        {
            log::warn!(
                "dayz service v{} does not match the v{} this plugin was built for",
                service.version,
                crate::VERSION
            );
            return None;
        }
        Some(Dayz {
            api: service.table,
            owner: host.me().raw(),
        })
    }

    /// The game build the address database matched, `None` for an unknown build.
    #[must_use]
    pub fn build(&self) -> Option<String> {
        // SAFETY: the adapter's build string lives as long as the process.
        let build = unsafe { borrow((self.api.build)()) };
        (!build.is_empty()).then(|| build.to_owned())
    }

    /// Absolute address of a named symbol, for example `render.frame`. Always ask by name;
    /// never compile an address into a plugin.
    ///
    /// # Errors
    /// The symbol is not in the database, or did not resolve for this build.
    pub fn symbol(&self, name: &str) -> Result<*mut c_void, PluginError> {
        let mut out = 0u64;
        // SAFETY: `out` is a valid out-pointer and `name` lives for the call.
        check(unsafe { (self.api.symbol)(Str::new(name), &raw mut out) })
            .map_err(|_| PluginError::Message(format!("{name} is not resolved for this build")))?;
        usize::try_from(out)
            .map(|address| address as *mut c_void)
            .map_err(|_| PluginError::Message(format!("{name} does not fit an address")))
    }

    /// A named structure offset, for example `framebase.rotation`.
    ///
    /// # Errors
    /// The offset is not in the database for this build.
    pub fn offset(&self, name: &str) -> Result<u64, PluginError> {
        let mut out = 0u64;
        // SAFETY: as in `symbol`.
        check(unsafe { (self.api.offset)(Str::new(name), &raw mut out) }).map_err(|_| {
            PluginError::Message(format!("offset {name} is not known for this build"))
        })?;
        Ok(out)
    }

    /// Check that every one of `names` resolves, for a plugin that cannot work without them.
    /// Call it in `start` and return the error, so the plugin fails to start with a reason.
    ///
    /// # Errors
    /// Names every symbol that did not resolve.
    pub fn require(&self, names: &[&str]) -> Result<(), PluginError> {
        let missing: Vec<&str> = names
            .iter()
            .copied()
            .filter(|name| self.symbol(name).is_err())
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(PluginError::Message(format!(
                "not resolved for this game build: {}",
                missing.join(", ")
            )))
        }
    }

    /// Hear the game's streams in `listener`. The last call wins; [`GameMask::NONE`]
    /// unsubscribes.
    ///
    /// # Errors
    /// The adapter could not hook the engine on this build, in which case nothing will ever be
    /// delivered.
    pub fn listen<L: GameListener>(
        &self,
        streams: GameMask,
        listener: L,
    ) -> Result<(), PluginError> {
        let ctx = Box::into_raw(Box::new(listener));
        let sink = GameSink {
            struct_size: core::mem::size_of::<GameSink>(),
            ctx: ctx.cast(),
            on_event: Some(deliver_event::<L>),
            on_chat: Some(deliver_chat::<L>),
            on_rpc: Some(deliver_rpc::<L>),
        };
        // SAFETY: `sink` lives for the call and the adapter copies it; `ctx` matches the three
        // functions it is paired with.
        let status = unsafe { (self.api.listen)(self.owner, streams, &raw const sink) };
        if !status.is_ok() {
            // SAFETY: refused, so the adapter kept no copy of the pointer.
            drop(unsafe { Box::from_raw(ctx) });
        }
        check(status)
    }

    /// Every event class the adapter knows the shape of, sorted by name. An empty list means
    /// this build's database has no event section; events are still delivered, named.
    ///
    /// # Errors
    /// The adapter refused the call.
    pub fn catalogue(&self) -> Result<Vec<Class>, PluginError> {
        let blank = GameClass {
            struct_size: core::mem::size_of::<GameClass>(),
            name: Str::EMPTY,
            note: Str::EMPTY,
            field_count: 0,
        };
        let mut slots = vec![blank; CATALOGUE_GUESS];
        let mut total = 0usize;
        // SAFETY: `slots` is writable for `slots.len()` structures and `total` is writable.
        let status =
            unsafe { (self.api.catalogue)(slots.as_mut_ptr(), slots.len(), &raw mut total) };
        if total > slots.len() {
            slots = vec![blank; total];
            // SAFETY: as above, with the room the adapter asked for.
            check(unsafe {
                (self.api.catalogue)(slots.as_mut_ptr(), slots.len(), &raw mut total)
            })?;
        } else {
            check(status)?;
        }
        Ok(slots
            .iter()
            .take(total)
            .map(|slot| Class {
                // SAFETY: catalogue strings last as long as the process.
                name: unsafe { borrow(slot.name) },
                // SAFETY: as above.
                note: unsafe { borrow(slot.note) },
                field_count: slot.field_count,
            })
            .collect())
    }

    /// Put one line in this client's own chat, which nobody else sees, on the next frame.
    /// `colour` names a colour class such as `ColorFriendly`; `""` takes the game's default.
    ///
    /// # Errors
    /// The game object is not known yet: it arrives once a session has started.
    pub fn chat_local(&self, text: &str, colour: &str) -> Result<(), PluginError> {
        // SAFETY: both strings live for the call.
        check(unsafe { (self.api.chat_local)(Str::new(text), Str::new(colour)) })
    }

    /// The engine's `CGame` instance, once a session has started.
    #[must_use]
    pub fn game_instance(&self) -> Option<*mut c_void> {
        // SAFETY: no arguments.
        let game = unsafe { (self.api.game_instance)() };
        (!game.is_null()).then_some(game)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_matches_its_own_class_name() {
        let event = Event {
            name: "RespawnEvent",
            fields: &[],
            object: core::ptr::null_mut(),
        };
        assert!(event.is("RespawnEvent"));
        assert!(!event.is("Respawn"));
    }

    #[test]
    fn an_empty_or_null_string_borrows_as_empty() {
        // SAFETY: an empty `Str` is valid by definition.
        assert_eq!(unsafe { borrow(Str::EMPTY) }, "");
        let text = "ColorFriendly";
        // SAFETY: `text` outlives the borrow.
        assert_eq!(unsafe { borrow(Str::new(text)) }, text);
    }

    #[test]
    fn a_panicking_listener_falls_back() {
        let verdict = contained("chat", GameResponse::PASS, || -> GameResponse {
            panic!("listener bug")
        });
        assert_eq!(verdict, GameResponse::PASS);
    }
}
