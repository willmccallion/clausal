//! Learned-clause deletion policy.

use crate::context::{ClauseRef, SearchContext};

/// Decides which learned clauses to discard during a reduction sweep.
pub trait ClauseDeletion: Send + 'static {
    /// A short human-readable name.
    fn name(&self) -> &'static str;

    /// Returns `true` if the next reduction sweep should run.
    fn should_reduce(&mut self, ctx: &SearchContext<'_>) -> bool;

    /// Returns `true` if the given learned clause should be discarded.
    fn should_delete(&mut self, ctx: &SearchContext<'_>, clause: &ClauseRef<'_>) -> bool;

    /// Called after a reduction sweep completes.
    fn on_reduced(&mut self, _ctx: &SearchContext<'_>) {}
}
