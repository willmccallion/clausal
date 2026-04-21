//! Conflict History Based heuristic.

use alloc::vec::Vec;

use crate::context::SearchContext;
use crate::traits::DecisionHeuristic;
use crate::types::Lit;

/// CHB branching.
#[derive(Debug)]
pub struct Chb {
    multiplier: f64,
    scores: Vec<f64>,
}

impl Chb {
    /// Creates a CHB heuristic with the given reward multiplier.
    #[must_use]
    pub const fn new(multiplier: f64) -> Self {
        Self { multiplier, scores: Vec::new() }
    }
}

impl Default for Chb {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl DecisionHeuristic for Chb {
    fn name(&self) -> &'static str {
        "chb"
    }

    fn pick_branch(&mut self, _ctx: &SearchContext<'_>) -> Option<Lit> {
        let _ = (self.multiplier, &self.scores);
        None
    }
}
