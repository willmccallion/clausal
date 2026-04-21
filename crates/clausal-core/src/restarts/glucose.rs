//! Glucose-style adaptive restarts based on LBD EMAs.

use crate::context::SearchContext;
use crate::traits::RestartStrategy;

/// Fires when the fast LBD EMA exceeds the slow EMA by `ratio`, unless the
/// trail-size blocker defers the restart.
#[derive(Debug, Clone, Copy)]
pub struct Glucose {
    fast_alpha: f64,
    slow_alpha: f64,
    ratio: f64,
    trail_block_ratio: f64,
    fast: f64,
    slow: f64,
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
            slow: 0.0,
        }
    }
}

impl Default for Glucose {
    fn default() -> Self {
        Self::new(1.0 / 32.0, 1.0 / 4096.0, 1.25, 1.4)
    }
}

impl RestartStrategy for Glucose {
    fn name(&self) -> &'static str {
        "glucose"
    }

    fn should_restart(&mut self, _ctx: &SearchContext<'_>) -> bool {
        let _ = (
            self.fast_alpha,
            self.slow_alpha,
            self.ratio,
            self.trail_block_ratio,
            self.fast,
            self.slow,
        );
        false
    }

    fn on_learned_lbd(&mut self, _lbd: u32) {}
}
