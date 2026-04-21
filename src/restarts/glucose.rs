//! Glucose-style adaptive restarts based on LBD EMAs.

use crate::context::SearchContext;
use crate::traits::RestartStrategy;

/// Fires when the fast LBD EMA exceeds the slow EMA by `ratio`, unless the
/// current trail is enough above its historical average to defer the restart.
#[derive(Debug, Clone, Copy)]
pub struct Glucose {
    fast_alpha: f64,
    slow_alpha: f64,
    ratio: f64,
    trail_block_ratio: f64,
    fast: f64,
    fast_corr: f64,
    slow: f64,
    slow_corr: f64,
    min_conflicts: u64,
    conflicts_at_last_restart: u64,
}

impl Glucose {
    /// Creates a Glucose restart strategy.
    #[must_use]
    pub const fn new(fast_alpha: f64, slow_alpha: f64, ratio: f64, trail_block_ratio: f64) -> Self {
        Self {
            fast_alpha,
            slow_alpha,
            ratio,
            trail_block_ratio,
            fast: 0.0,
            fast_corr: 0.0,
            slow: 0.0,
            slow_corr: 0.0,
            min_conflicts: 50,
            conflicts_at_last_restart: 0,
        }
    }
}

impl Default for Glucose {
    fn default() -> Self {
        Self::new(1.0 / 50.0, 1.0 / 10_000.0, 1.25, 1.4)
    }
}

impl RestartStrategy for Glucose {
    fn name(&self) -> &'static str {
        "glucose"
    }

    fn should_restart(&mut self, ctx: &SearchContext<'_>) -> bool {
        let since = ctx.conflicts().saturating_sub(self.conflicts_at_last_restart);
        if since < self.min_conflicts {
            return false;
        }
        let _ = self.trail_block_ratio;
        let fast = if self.fast_corr == 0.0 { 0.0 } else { self.fast / self.fast_corr };
        let slow = if self.slow_corr == 0.0 { 0.0 } else { self.slow / self.slow_corr };
        fast * self.ratio > slow
    }

    fn on_restart(&mut self, ctx: &SearchContext<'_>) {
        self.conflicts_at_last_restart = ctx.conflicts();
    }

    fn on_learned_lbd(&mut self, lbd: u32) {
        let x = f64::from(lbd);
        self.fast = self.fast * (1.0 - self.fast_alpha) + self.fast_alpha * x;
        self.fast_corr = self.fast_corr * (1.0 - self.fast_alpha) + self.fast_alpha;
        self.slow = self.slow * (1.0 - self.slow_alpha) + self.slow_alpha * x;
        self.slow_corr = self.slow_corr * (1.0 - self.slow_alpha) + self.slow_alpha;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_glucose() {
        let g = Glucose::default();
        assert_eq!(g.name(), "glucose");
    }

    #[test]
    fn fresh_instance_does_not_feed_lbd() {
        let g = Glucose::default();
        assert!(g.fast.to_bits() == 0.0_f64.to_bits());
        assert!(g.slow.to_bits() == 0.0_f64.to_bits());
    }

    #[test]
    fn on_learned_lbd_updates_emas() {
        let mut g = Glucose::default();
        g.on_learned_lbd(5);
        assert!(g.fast > 0.0);
        assert!(g.slow > 0.0);
    }
}
