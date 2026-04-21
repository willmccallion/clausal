//! IPASIR C ABI compatibility layer for [`clausal`](../clausal/).
//!
//! Early scaffolding. Every `ipasir_*` function currently returns a
//! documented sentinel and sets a thread-local error flag reachable via
//! [`ipasir_failed`].
