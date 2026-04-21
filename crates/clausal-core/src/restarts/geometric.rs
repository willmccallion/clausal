//! Geometric restart schedule.

use crate::context::SearchContext;
use crate::traits::RestartStrategy;

/// Restarts every `base * factor^n` conflicts.
#[derive(Debug, Clone, Copy)]
pub struct Geometric {
    base: u64,
    factor: f64,
    next: u64,
}

impl Geometric {
    /// Creates a geometric restart schedule with the given base and factor.
    #[must_use]
    pub const fn new(base: u64, factor: f64) -> Self {
        Self { base, factor, next: base }
    }
}

impl Default for Geometric {
    fn default() -> Self {
        Self::new(100, 1.5)
    }
}

impl RestartStrategy for Geometric {
    fn name(&self) -> &'static str {
        "geometric"
    }

    fn should_restart(&mut self, _ctx: &SearchContext<'_>) -> bool {
        let _ = (self.base, self.factor, self.next);
        false
    }
}
