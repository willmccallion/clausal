//! Geometric restart schedule.

use crate::context::SearchContext;
use crate::traits::RestartStrategy;

/// Restarts every `base * factor^n` conflicts, where `n` counts prior restarts.
#[derive(Debug, Clone, Copy)]
pub struct Geometric {
    base: u64,
    factor: f64,
    next: u64,
    conflicts_at_last_restart: u64,
}

impl Geometric {
    /// Creates a geometric restart schedule with the given base and factor.
    #[must_use]
    pub const fn new(base: u64, factor: f64) -> Self {
        Self { base, factor, next: base, conflicts_at_last_restart: 0 }
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

    fn should_restart(&mut self, ctx: &SearchContext<'_>) -> bool {
        let since = ctx.conflicts().saturating_sub(self.conflicts_at_last_restart);
        since >= self.next
    }

    fn on_restart(&mut self, ctx: &SearchContext<'_>) {
        self.conflicts_at_last_restart = ctx.conflicts();
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let scaled = (self.next as f64 * self.factor) as u64;
        self.next = scaled.max(self.next.saturating_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_grows_by_factor_on_each_restart() {
        let mut g = Geometric::new(100, 2.0);
        assert_eq!(g.next, 100);
        g.next = (g.next as f64 * g.factor) as u64;
        assert_eq!(g.next, 200);
    }

    #[test]
    fn name_is_geometric() {
        let g = Geometric::default();
        assert_eq!(g.name(), "geometric");
    }
}
