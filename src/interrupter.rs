//! [`Interrupter`]: a thread-safe handle for asking a running solver to stop.
//!
//! Requires atomics on the target platform. On targets without atomic
//! pointer or atomic u8 support the module is compiled out and
//! `Solver::interrupter()` returns [`Error::AtomicsUnavailable`].
//!
//! [`Error::AtomicsUnavailable`]: crate::error::Error::AtomicsUnavailable

#![cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))]

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

/// A clone-safe handle for interrupting a running solver.
///
/// Each clone shares the same underlying atomic flag. Calling
/// [`Interrupter::interrupt`] from any clone signals every solver that
/// holds a sibling clone.
#[derive(Clone, Debug)]
#[must_use]
pub struct Interrupter {
    flag: Arc<AtomicBool>,
}

impl Interrupter {
    /// Creates a fresh interrupter in the un-interrupted state.
    #[inline]
    pub fn new() -> Self {
        Self { flag: Arc::new(AtomicBool::new(false)) }
    }

    /// Signals every sibling clone that the solver should stop.
    #[inline]
    pub fn interrupt(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Returns `true` once [`Self::interrupt`] has been called.
    #[inline]
    #[must_use]
    pub fn is_interrupted(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }
}

impl Default for Interrupter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_not_interrupted() {
        assert!(!Interrupter::new().is_interrupted());
    }

    #[test]
    fn clones_share_state() {
        let a = Interrupter::new();
        let b = a.clone();
        assert!(!b.is_interrupted());
        a.interrupt();
        assert!(b.is_interrupted());
    }
}
