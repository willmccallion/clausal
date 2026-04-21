//! Variable State Independent Decaying Sum heuristic.

use alloc::vec::Vec;

use crate::context::SearchContext;
use crate::traits::DecisionHeuristic;
use crate::types::Lit;

/// VSIDS with activity decay and a binary max-heap.
///
/// Stub: fields are present so the engine can land without restructuring,
/// but [`Self::pick_branch`] currently returns `None`.
#[derive(Debug)]
pub struct Vsids {
    activities: Vec<f64>,
    inc: f64,
    decay: f64,
    heap: Vec<u32>,
    pos: Vec<u32>,
}

impl Vsids {
    /// Creates a VSIDS heuristic with the given activity decay factor.
    #[must_use]
    pub const fn new(decay: f64) -> Self {
        Self {
            activities: Vec::new(),
            inc: 1.0,
            decay,
            heap: Vec::new(),
            pos: Vec::new(),
        }
    }
}

impl Default for Vsids {
    fn default() -> Self {
        Self::new(0.95)
    }
}

impl DecisionHeuristic for Vsids {
    fn name(&self) -> &'static str {
        "vsids"
    }

    fn pick_branch(&mut self, _ctx: &SearchContext<'_>) -> Option<Lit> {
        let _ = (&self.activities, self.inc, self.decay, &self.heap, &self.pos);
        None
    }
}
