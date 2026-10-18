//! The `dayz` service table.
//!
//! Each function is a thin translation from the ABI to the modules that do the work. None of
//! them can panic: every lock tolerates poisoning and nothing unwraps.

// FFI module: the table's functions take raw pointers from other plugins.
#![allow(unsafe_code)]

use core::ffi::c_void;

use dayz_game::{DayzApi, GameClass, GameMask, GameSink};
use uniloader_sdk::api::{PluginHandle, Status, Str};

use super::{data, event_fields, game_events, session};

/// The table registered as the service `dayz`, version [`dayz_game::VERSION`].
pub(crate) static TABLE: DayzApi = DayzApi {
    struct_size: core::mem::size_of::<DayzApi>(),
    build,
    symbol,
    offset,
    listen,
    catalogue,
    chat_local,
    game_instance,
};

/// Read a name a caller passed.
///
/// # Safety
/// `s` must describe readable bytes for the call, as the ABI requires.
unsafe fn text<'a>(s: Str) -> &'a str {
    if s.ptr.is_null() || s.len == 0 {
        return "";
    }
    // SAFETY: the caller's contract.
    let bytes = unsafe { core::slice::from_raw_parts(s.ptr, s.len) };
    core::str::from_utf8(bytes).unwrap_or("")
}

unsafe extern "C" fn build() -> Str {
    data::resolved().map_or(Str::EMPTY, |resolved| Str::new(&resolved.build))
}

unsafe extern "C" fn symbol(name: Str, out: *mut u64) -> Status {
    if out.is_null() {
        return Status::INVALID_ARGUMENT;
    }
    let Some(resolved) = data::resolved() else {
        return Status::NOT_FOUND;
    };
    // SAFETY: the ABI makes `name` valid for the call.
    let name = unsafe { text(name) };
    let Some(entry) = resolved.table.symbol(name) else {
        return Status::NOT_FOUND;
    };
    let Some(address) = (resolved.module_base as u64).checked_add(entry.rva) else {
        return Status::NOT_FOUND;
    };
    // SAFETY: checked non-null; the caller provides a writable `u64`.
    unsafe { out.write(address) };
    Status::OK
}

unsafe extern "C" fn offset(name: Str, out: *mut u64) -> Status {
    if out.is_null() {
        return Status::INVALID_ARGUMENT;
    }
    // SAFETY: the ABI makes `name` valid for the call.
    let name = unsafe { text(name) };
    let Some(value) = data::resolved().and_then(|resolved| resolved.table.offset(name)) else {
        return Status::NOT_FOUND;
    };
    // SAFETY: checked non-null; the caller provides a writable `u64`.
    unsafe { out.write(value) };
    Status::OK
}

unsafe extern "C" fn listen(owner: PluginHandle, mask: GameMask, sink: *const GameSink) -> Status {
    let sink = if sink.is_null() {
        None
    } else {
        // SAFETY: the ABI makes a non-null `sink` readable for the call; it is copied here.
        let sink = unsafe { &*sink };
        (sink.struct_size >= core::mem::size_of::<GameSink>()).then_some(*sink)
    };
    game_events::listen(owner, mask, sink)
}

unsafe extern "C" fn catalogue(out: *mut GameClass, capacity: usize, total: *mut usize) -> Status {
    let slots: &mut [GameClass] = if out.is_null() || capacity == 0 {
        &mut []
    } else {
        // SAFETY: the ABI makes `out` writable for `capacity` structures.
        unsafe { core::slice::from_raw_parts_mut(out, capacity) }
    };
    let count = event_fields::catalogue(slots);
    if !total.is_null() {
        // SAFETY: checked non-null; the caller provides a writable `usize`.
        unsafe { total.write(count) };
    }
    if count > capacity {
        Status::BUFFER_TOO_SMALL
    } else {
        Status::OK
    }
}

unsafe extern "C" fn chat_local(text_in: Str, colour: Str) -> Status {
    // SAFETY: both strings are valid for the call; they are copied into the queue.
    let (line, colour) = unsafe { (text(text_in), text(colour)) };
    match session::request_chat(line, colour) {
        Ok(()) => Status::OK,
        Err(why) => {
            log::debug!("local chat refused: {why}");
            Status::NOT_FOUND
        }
    }
}

unsafe extern "C" fn game_instance() -> *mut c_void {
    game_events::game_instance().unwrap_or(core::ptr::null_mut())
}
