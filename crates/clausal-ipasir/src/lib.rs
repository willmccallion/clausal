//! IPASIR C ABI compatibility layer for [`clausal`](../clausal/).
//!
//! Exposes the standard `ipasir_*` symbols so C and C++ callers can link
//! against clausal via the IPASIR interface.

mod ffi;

pub use ffi::IPASIR_SENTINEL;
