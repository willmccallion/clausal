//! Inprocessing passes invoked outside the hot CDCL loop.
//!
//! Each pass runs at the ground decision level with boolean constraint
//! propagation already at a fixed point. Passes derive new units, shrink
//! long clauses, or eliminate variables; all return an
//! [`InprocessOutcome`] reporting whether the formula has become
//! unsatisfiable during the pass.
//!
//! The current integration point is a single pre-search sweep driven from
//! [`crate::solver::state::SolverState::run_inprocessing`] on solver
//! construction. That layout keeps the mid-search path identical to the
//! Stage 2-through-8 engine, at the cost of not interleaving inprocessing
//! with conflict-driven learning. Future work can move the dispatch into
//! the post-conflict cadence block of the main loop.

pub(crate) mod bve;
pub(crate) mod equiv;
pub(crate) mod probe;
pub(crate) mod subsume;
pub(crate) mod vivify;

/// Result of a single inprocessing pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InprocessOutcome {
    /// Search should continue; the formula may have been simplified.
    Continue,
    /// The pass discovered the formula is unsatisfiable at the ground level.
    Unsat,
}
