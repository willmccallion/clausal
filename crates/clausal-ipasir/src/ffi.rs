//! C ABI symbol exports for the IPASIR interface.
//!
//! The only use of `unsafe` in this crate is the `#[unsafe(no_mangle)]`
//! attribute required by edition 2024 to export symbols under their exact
//! C name. No function body contains an `unsafe` block; raw pointers are
//! accepted as opaque handles and never dereferenced.
#![allow(
    unsafe_code,
    reason = "C ABI symbol export via #[unsafe(no_mangle)] only; no unsafe blocks, no pointer dereference"
)]

use core::cell::Cell;
use core::ffi::{c_char, c_int, c_void};
use core::ptr;

thread_local! {
    static LAST_ERROR: Cell<c_int> = const { Cell::new(0) };
}

/// Value returned by every entry point to signal that no engine is wired.
pub const IPASIR_SENTINEL: c_int = -1;

fn set_error(code: c_int) {
    LAST_ERROR.with(|e| e.set(code));
}

/// Returns a NUL-terminated signature identifying the solver and version.
#[unsafe(no_mangle)]
pub extern "C" fn ipasir_signature() -> *const c_char {
    static SIG: &[u8] = b"clausal-0.1.0\0";
    SIG.as_ptr().cast()
}

/// Allocates a solver instance and returns an opaque handle.
#[unsafe(no_mangle)]
pub extern "C" fn ipasir_init() -> *mut c_void {
    set_error(IPASIR_SENTINEL);
    ptr::null_mut()
}

/// Releases a solver instance previously returned by [`ipasir_init`].
#[unsafe(no_mangle)]
pub extern "C" fn ipasir_release(_solver: *mut c_void) {
    set_error(IPASIR_SENTINEL);
}

/// Appends a literal to the current clause; a zero literal terminates it.
#[unsafe(no_mangle)]
pub extern "C" fn ipasir_add(_solver: *mut c_void, _lit_or_zero: i32) {
    set_error(IPASIR_SENTINEL);
}

/// Adds an assumption literal for the next solve call.
#[unsafe(no_mangle)]
pub extern "C" fn ipasir_assume(_solver: *mut c_void, _lit: i32) {
    set_error(IPASIR_SENTINEL);
}

/// Drives search. Returns 10 for SAT, 20 for UNSAT, 0 for interrupted.
#[unsafe(no_mangle)]
pub extern "C" fn ipasir_solve(_solver: *mut c_void) -> c_int {
    set_error(IPASIR_SENTINEL);
    0
}

/// Returns the assigned value of a literal in the most recent model.
#[unsafe(no_mangle)]
pub extern "C" fn ipasir_val(_solver: *mut c_void, _lit: i32) -> i32 {
    set_error(IPASIR_SENTINEL);
    0
}

/// Returns nonzero if the given assumption was used to derive UNSAT.
#[unsafe(no_mangle)]
pub extern "C" fn ipasir_failed(_solver: *mut c_void, _lit: i32) -> c_int {
    LAST_ERROR.with(Cell::get)
}

/// Installs a termination callback polled during search.
#[unsafe(no_mangle)]
pub extern "C" fn ipasir_set_terminate(
    _solver: *mut c_void,
    _data: *mut c_void,
    _cb: Option<extern "C" fn(*mut c_void) -> c_int>,
) {
    set_error(IPASIR_SENTINEL);
}

/// Installs a learned-clause callback invoked on clauses up to `max_len`.
#[unsafe(no_mangle)]
pub extern "C" fn ipasir_set_learn(
    _solver: *mut c_void,
    _data: *mut c_void,
    _max_len: c_int,
    _cb: Option<extern "C" fn(*mut c_void, *const i32)>,
) {
    set_error(IPASIR_SENTINEL);
}
