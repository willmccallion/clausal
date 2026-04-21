//! Restart policy trait.

use crate::context::SearchContext;

/// Decides when to restart search from the root.
pub trait RestartStrategy: Send + 'static {
    /// A short human-readable name.
    fn name(&self) -> &'static str;

    /// Returns `true` if the solver should restart now.
    fn should_restart(&mut self, ctx: &SearchContext<'_>) -> bool;

    /// Called after a restart has taken place so the strategy can reset
    /// internal counters.
    fn on_restart(&mut self, _ctx: &SearchContext<'_>) {}

    /// Feeds the latest learned-clause LBD into the strategy.
    fn on_learned_lbd(&mut self, _lbd: u32) {}
}
