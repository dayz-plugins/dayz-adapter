//! A structured exception handler around one raw read.
//!
//! The loader's own guard would catch a fault too, but it would blame this plugin and disable
//! it. A `read` of a bad address typed into the console is the user's mistake, not the
//! adapter's, so the copy gets a handler of its own and the fault becomes a console line.

// FFI module: runs a closure under a structured exception handler.
#![allow(unsafe_code)]

use std::fmt;

/// A hardware exception caught inside [`call`].
#[derive(Debug, Clone, Copy)]
pub(super) struct Fault(u32);

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exception {:#010X}", self.0)
    }
}

/// Run `f`, turning an access violation inside it into an error.
///
/// # Safety
/// `f` may fault, which is the point, but it must not leave anything half-written that the
/// caller relies on: a fault abandons it at an arbitrary instruction.
#[cfg(target_env = "msvc")]
pub(super) unsafe fn call(f: impl FnOnce()) -> Result<(), Fault> {
    let mut once = Some(f);
    microseh::try_seh(|| {
        if let Some(f) = once.take() {
            f();
        }
    })
    .map_err(|e| Fault(e.code() as u32))
}

/// Without MSVC there is no structured exception handling to call; the read happens bare,
/// after the same `VirtualQuery` check, and a fault in the gap takes the loader's guard.
///
/// # Safety
/// As for the MSVC version.
#[cfg(not(target_env = "msvc"))]
pub(super) unsafe fn call(f: impl FnOnce()) -> Result<(), Fault> {
    f();
    Ok(())
}
