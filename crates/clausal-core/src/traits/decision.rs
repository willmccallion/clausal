//! Branch variable selection heuristic.

use crate::context::SearchContext;
use crate::types::{DecisionLevel, Lit};

/// Selects which literal to branch on next.
///
/// Implementors observe assign/unassign/conflict/learned events and return a
/// literal from [`Self::pick_branch`]. Returning `None` signals that every
/// variable is assigned; the solver then checks for a satisfying model.
pub trait DecisionHeuristic: Send + 'static {
    /// A short human-readable name. Appears in statistics and logs.
    fn name(&self) -> &'static str;

    /// Chooses the next literal to branch on.
    fn pick_branch(&mut self, ctx: &SearchContext<'_>) -> Option<Lit>;

    /// Called when a literal is assigned on the trail.
    fn on_assign(&mut self, _lit: Lit, _level: DecisionLevel) {}

    /// Called when a literal is unassigned during backtrack.
    fn on_unassign(&mut self, _lit: Lit) {}

    /// Called when conflict analysis starts.
    fn on_conflict(&mut self, _ctx: &SearchContext<'_>) {}

    /// Called when a clause is learned.
    fn on_learned(&mut self, _ctx: &SearchContext<'_>, _clause: &[Lit]) {}
}
