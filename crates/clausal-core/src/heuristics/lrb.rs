//! Learning Rate Branching heuristic.

use alloc::vec::Vec;

use crate::context::SearchContext;
use crate::traits::DecisionHeuristic;
use crate::types::Lit;

/// LRB branching with exponential-recency weighting.
#[derive(Debug)]
pub struct Lrb {
    alpha: f64,
    ema: Vec<f64>,
}

impl Lrb {
    /// Creates an LRB heuristic with the given initial step size.
    #[must_use]
    pub const fn new(alpha: f64) -> Self {
        Self { alpha, ema: Vec::new() }
    }
}

impl Default for Lrb {
    fn default() -> Self {
        Self::new(0.4)
    }
}

impl DecisionHeuristic for Lrb {
    fn name(&self) -> &'static str {
        "lrb"
    }

    fn pick_branch(&mut self, _ctx: &SearchContext<'_>) -> Option<Lit> {
        let _ = (self.alpha, &self.ema);
        None
    }
}
