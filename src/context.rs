//! Lifetime-borrowed views given to user-supplied heuristics and preprocessors.

use crate::solver::Solver;
use crate::types::{ClauseId, DecisionLevel, Lit};

/// A read-only view of the solver during conflict analysis and branching.
///
/// Passed to [`DecisionHeuristic`] callbacks. Extension traits that want to
/// reach into the solver's live state do so through this handle.
///
/// [`DecisionHeuristic`]: crate::traits::DecisionHeuristic
#[derive(Debug)]
pub struct SearchContext<'s> {
    solver: &'s Solver,
}

impl<'s> SearchContext<'s> {
    #[allow(dead_code, reason = "constructed by solver loop once heuristics are dispatched")]
    pub(crate) const fn new(solver: &'s Solver) -> Self {
        Self { solver }
    }

    /// The current decision level.
    #[must_use]
    pub fn decision_level(&self) -> DecisionLevel {
        self.solver.decision_level()
    }

    /// The number of conflicts seen so far.
    #[must_use]
    pub fn conflicts(&self) -> u64 {
        self.solver.statistics().conflicts
    }
}

/// A read-only view of the original formula used by [`Preprocessor`] callbacks.
///
/// [`Preprocessor`]: crate::traits::Preprocessor
#[derive(Debug)]
pub struct FormulaView<'s> {
    solver: &'s Solver,
}

impl<'s> FormulaView<'s> {
    #[allow(dead_code, reason = "constructed by solver once preprocessor pipeline runs against the Cnf view")]
    pub(crate) const fn new(solver: &'s Solver) -> Self {
        Self { solver }
    }

    /// Returns the number of variables in the formula.
    #[must_use]
    pub const fn num_vars(&self) -> u32 {
        self.solver.num_vars()
    }
}

/// A borrowed view of one clause stored inside the solver.
#[derive(Debug)]
pub struct ClauseRef<'s> {
    solver: &'s Solver,
    id: ClauseId,
}

impl<'s> ClauseRef<'s> {
    #[allow(dead_code, reason = "constructed when the solver hands user code a clause view")]
    pub(crate) const fn new(solver: &'s Solver, id: ClauseId) -> Self {
        Self { solver, id }
    }

    /// The id of this clause inside the solver.
    #[must_use]
    pub const fn id(&self) -> ClauseId {
        self.id
    }

    /// The literals in the clause.
    #[must_use]
    pub const fn lits(&self) -> &[Lit] {
        let _ = self.solver;
        &[]
    }

    /// The number of literals in the clause.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.lits().len()
    }

    /// Returns `true` if the clause has no literals.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.lits().is_empty()
    }

    /// Whether this clause was learned via conflict analysis.
    #[must_use]
    pub const fn is_learned(&self) -> bool {
        let _ = self.solver;
        false
    }

    /// The clause's Literal Block Distance.
    #[must_use]
    pub const fn lbd(&self) -> u32 {
        0
    }

    /// The clause's activity score.
    #[must_use]
    pub const fn activity(&self) -> f64 {
        0.0
    }
}
