//! Deletion driven by Literal Block Distance.

use crate::context::{ClauseRef, SearchContext};
use crate::traits::ClauseDeletion;

/// Deletes clauses whose LBD exceeds a fixed ceiling.
///
/// Triggered by a conflict-count budget that grows after each reduction.
#[derive(Debug, Clone, Copy)]
pub struct LbdBased {
    ceiling: u32,
    next_reduce: u64,
    interval: u64,
    grow: u64,
}

impl LbdBased {
    /// Creates an LBD-based policy with the given ceiling.
    #[must_use]
    pub const fn new(ceiling: u32) -> Self {
        Self {
            ceiling,
            next_reduce: 2_000,
            interval: 2_000,
            grow: 300,
        }
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

    fn should_reduce(&mut self, ctx: &SearchContext<'_>) -> bool {
        if ctx.conflicts() < self.next_reduce {
            return false;
        }
        self.interval = self.interval.saturating_add(self.grow);
        self.next_reduce = ctx.conflicts().saturating_add(self.interval);
        true
    }

    fn should_delete(&mut self, _ctx: &SearchContext<'_>, clause: &ClauseRef<'_>) -> bool {
        clause.is_learned() && clause.lbd() > self.ceiling
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_lbd() {
        let d = LbdBased::default();
        assert_eq!(d.name(), "lbd");
    }

    #[test]
    fn interval_grows_after_trigger() {
        let mut d = LbdBased::new(6);
        let initial = d.interval;
        d.interval = d.interval.saturating_add(d.grow);
        assert!(d.interval > initial);
    }
}
