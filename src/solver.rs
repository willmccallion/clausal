//! [`Solver`]: the incremental CDCL solver entry point.

pub(crate) mod mode;
pub(crate) mod order_heap;
pub(crate) mod reduce;
pub(crate) mod rephase;
pub(crate) mod restart;
pub(crate) mod search;
pub(crate) mod state;

use alloc::vec::Vec;

use crate::builder::SolverBuilder;
use crate::error::{Error, Result};
use crate::result::{Limited, Model, Solution, Solutions, UnsatCore};
use crate::solver::search::search_loop::SearchOutcome;
use crate::solver::state::{LastOutcome, SolverState};
use crate::stats::Statistics;
use crate::types::{Clause, DecisionLevel, Lit, Value, Var};

#[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))]
use crate::interrupter::Interrupter;

/// An incremental CDCL SAT solver.
///
/// Construct one with [`Solver::new`] or configure via [`SolverBuilder`].
/// Append clauses with [`Solver::add`] or [`Solver::add_clause`], then call
/// [`Solver::solve`] or [`Solver::solve_under`] to drive search.
#[derive(Debug, Default)]
#[must_use]
pub struct Solver {
    pending: Vec<Clause>,
    state: SolverState,
}

impl Solver {
    /// Creates a solver with default configuration.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self { pending: Vec::new(), state: SolverState::new() }
    }

    /// Returns a builder for configuring a new solver.
    #[inline]
    pub fn builder() -> SolverBuilder {
        SolverBuilder::new()
    }

    /// Allocates one fresh variable.
    pub fn new_var(&mut self) -> Result<Var> {
        if self.state.num_vars >= Var::MAX_RAW {
            return Err(Error::VariableLimitExceeded);
        }
        let next = self.state.num_vars.saturating_add(1);
        self.state.grow_to(next);
        Var::new(next).ok_or(Error::VariableLimitExceeded)
    }

    /// Allocates `count` fresh variables.
    pub fn new_vars(&mut self, count: usize) -> Result<Vec<Var>> {
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            out.push(self.new_var()?);
        }
        Ok(out)
    }

    /// Appends a clause built from an iterator of literals.
    pub fn add<I: IntoIterator<Item = Lit>>(&mut self, lits: I) {
        self.pending.push(Clause::from_lits(lits));
    }

    /// Appends an already-built clause.
    pub fn add_clause(&mut self, clause: Clause) {
        self.pending.push(clause);
    }

    /// Returns the number of variables known to the solver.
    #[inline]
    #[must_use]
    pub const fn num_vars(&self) -> u32 {
        self.state.num_vars
    }

    /// Returns the number of clauses currently held by the solver.
    #[inline]
    #[must_use]
    pub fn num_clauses(&self) -> usize {
        self.pending.len() + self.state.arena.num_clauses()
    }

    /// Returns the solver's accumulated statistics.
    #[inline]
    #[must_use]
    pub fn statistics(&self) -> Statistics {
        Statistics {
            decisions: 0,
            conflicts: self.state.scratch.conflicts,
            propagations: 0,
            restarts: self.state.scratch.restarts,
            learned: self.state.learned_clauses.len() as u64,
            removed: 0,
            variables: u64::from(self.state.num_vars),
            clauses: self.num_clauses() as u64,
        }
    }

    /// Returns the solver's current decision level.
    #[inline]
    #[must_use]
    pub fn decision_level(&self) -> DecisionLevel {
        self.state.assignment.current_level()
    }

    /// Returns the current three-valued truth value of the given literal.
    #[must_use]
    pub fn value(&self, lit: Lit) -> Value {
        if lit.var().to_raw() > self.state.num_vars {
            return Value::Unassigned;
        }
        self.state.assignment.value_of(lit)
    }

    /// Returns an interrupter handle.
    #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))]
    pub fn interrupter(&self) -> Result<Interrupter> {
        Err(Error::NotImplemented)
    }

    /// Returns an interrupter handle. Always fails on targets without atomics.
    #[cfg(not(all(target_has_atomic = "8", target_has_atomic = "ptr")))]
    pub fn interrupter(&self) -> Result<()> {
        Err(Error::AtomicsUnavailable)
    }

    /// Drives search until a conclusion is reached.
    ///
    /// # Errors
    ///
    /// Currently never returns an error on the unbounded path; the `Result`
    /// wrapping is kept for forward compatibility with proof-emission
    /// failures and future resource guards.
    pub fn solve(&mut self) -> Result<Solution<'_>> {
        self.drain_pending();
        let outcome = self.state.solve();
        Ok(match outcome {
            SearchOutcome::Sat => Solution::Sat(Model::new(self)),
            SearchOutcome::Unsat => Solution::Unsat(UnsatCore::new(self)),
        })
    }

    /// Drives search under the given assumption literals.
    pub fn solve_under<I: IntoIterator<Item = Lit>>(&mut self, _assumptions: I) -> Result<Limited<'_>> {
        Err(Error::NotImplemented)
    }

    /// Returns the model for the most recent SAT result, if any.
    #[must_use]
    pub fn model(&self) -> Option<Model<'_>> {
        match self.state.last_outcome {
            LastOutcome::Sat => Some(Model::new(self)),
            _ => None,
        }
    }

    /// Returns the UNSAT core for the most recent UNSAT result, if any.
    #[must_use]
    pub fn unsat_core(&self) -> Option<UnsatCore<'_>> {
        match self.state.last_outcome {
            LastOutcome::Unsat => Some(UnsatCore::new(self)),
            _ => None,
        }
    }

    /// Returns an iterator over every satisfying assignment.
    pub fn solutions(&mut self) -> Solutions<'_> {
        Solutions::new(self)
    }

    #[doc(hidden)]
    pub fn var_polarity(&self, var: Var) -> crate::types::Polarity {
        match self.state.value(var) {
            Value::True => crate::types::Polarity::Positive,
            Value::False | Value::Unassigned => crate::types::Polarity::Negative,
        }
    }

    fn drain_pending(&mut self) {
        let pending = core::mem::take(&mut self.pending);
        for clause in &pending {
            let lits: Vec<Lit> = clause.iter().copied().collect();
            self.state.install_user_clause(&lits);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn new_solver_is_empty() {
        let s = Solver::new();
        assert_eq!(s.num_vars(), 0);
        assert_eq!(s.num_clauses(), 0);
        assert_eq!(s.decision_level(), DecisionLevel::GROUND);
    }

    #[test]
    fn solve_empty_is_sat() {
        let mut s = Solver::new();
        assert!(matches!(s.solve().unwrap(), Solution::Sat(_)));
    }

    #[test]
    fn solve_contradiction_is_unsat() {
        let mut s = Solver::new();
        let v1 = s.new_var().unwrap();
        s.add([v1.pos()]);
        s.add([v1.neg()]);
        assert!(matches!(s.solve().unwrap(), Solution::Unsat(_)));
    }

    #[test]
    fn new_var_bumps_count() {
        let mut s = Solver::new();
        let _ = s.new_var().unwrap();
        let _ = s.new_var().unwrap();
        assert_eq!(s.num_vars(), 2);
        assert_eq!(s.statistics().variables, 2);
    }

    #[test]
    fn model_available_after_sat() {
        let mut s = Solver::new();
        let vs = s.new_vars(3).unwrap();
        s.add([vs[0].pos(), vs[1].pos()]);
        s.add([vs[2].pos()]);
        let _ = s.solve().unwrap();
        assert!(s.model().is_some());
        assert!(s.unsat_core().is_none());
    }

    #[test]
    fn unsat_core_available_after_unsat() {
        let mut s = Solver::new();
        let v1 = s.new_var().unwrap();
        s.add([v1.pos()]);
        s.add([v1.neg()]);
        let _ = s.solve().unwrap();
        assert!(s.unsat_core().is_some());
        assert!(s.model().is_none());
    }
}
