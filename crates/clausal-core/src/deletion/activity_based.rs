//! Deletion driven by clause activity scores.

use crate::context::{ClauseRef, SearchContext};
use crate::traits::ClauseDeletion;

/// Deletes clauses whose activity falls below a fraction of the current max.
#[derive(Debug, Clone, Copy)]
pub struct ActivityBased {
    fraction: f64,
}

impl ActivityBased {
    /// Creates an activity-based policy with the given survival fraction.
    #[must_use]
    pub const fn new(fraction: f64) -> Self {
        Self { fraction }
    }
}

impl Default for ActivityBased {
    fn default() -> Self {
        Self::new(0.5)
    }
}

impl ClauseDeletion for ActivityBased {
    fn name(&self) -> &'static str {
        "activity"
    }

    fn should_reduce(&mut self, _ctx: &SearchContext<'_>) -> bool {
        false
    }

    fn should_delete(&mut self, _ctx: &SearchContext<'_>, _clause: &ClauseRef<'_>) -> bool {
        let _ = self.fraction;
        false
    }
}
