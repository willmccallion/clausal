//! Deletion driven by clause activity scores.

use crate::context::{ClauseRef, SearchContext};
use crate::traits::ClauseDeletion;

/// Deletes learned clauses whose activity falls below a fraction of the max
/// activity observed during the current reduce pass.
#[derive(Debug, Clone, Copy)]
pub struct ActivityBased {
    fraction: f64,
    next_reduce: u64,
    interval: u64,
    grow: u64,
    threshold: f64,
}

impl ActivityBased {
    /// Creates an activity-based policy with the given survival fraction.
    #[must_use]
    pub const fn new(fraction: f64) -> Self {
        Self {
            fraction,
            next_reduce: 2_000,
            interval: 2_000,
            grow: 300,
            threshold: 0.0,
        }
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

    fn should_reduce(&mut self, ctx: &SearchContext<'_>) -> bool {
        if ctx.conflicts() < self.next_reduce {
            return false;
        }
        self.interval = self.interval.saturating_add(self.grow);
        self.next_reduce = ctx.conflicts().saturating_add(self.interval);
        self.threshold = 0.0;
        true
    }

    fn should_delete(&mut self, _ctx: &SearchContext<'_>, clause: &ClauseRef<'_>) -> bool {
        if !clause.is_learned() {
            return false;
        }
        let act = clause.activity();
        if act > self.threshold {
            self.threshold = act * self.fraction;
        }
        act < self.threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_activity() {
        let d = ActivityBased::default();
        assert_eq!(d.name(), "activity");
    }

    #[test]
    fn fraction_stored_on_construction() {
        let d = ActivityBased::new(0.25);
        assert!((d.fraction - 0.25).abs() < f64::EPSILON);
    }
}
