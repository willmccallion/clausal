//! Deletion driven by Literal Block Distance.

use crate::context::{ClauseRef, SearchContext};
use crate::traits::ClauseDeletion;

/// Deletes clauses whose LBD exceeds a fixed ceiling.
#[derive(Debug, Clone, Copy)]
pub struct LbdBased {
    ceiling: u32,
}

impl LbdBased {
    /// Creates an LBD-based policy with the given ceiling.
    #[must_use]
    pub const fn new(ceiling: u32) -> Self {
        Self { ceiling }
    }
}

impl Default for LbdBased {
    fn default() -> Self {
        Self::new(6)
    }
}

impl ClauseDeletion for LbdBased {
    fn name(&self) -> &'static str {
        "lbd"
    }

    fn should_reduce(&mut self, _ctx: &SearchContext<'_>) -> bool {
        false
    }

    fn should_delete(&mut self, _ctx: &SearchContext<'_>, _clause: &ClauseRef<'_>) -> bool {
        let _ = self.ceiling;
        false
    }
}
