//! [`Solver`]: the incremental CDCL solver entry point.

pub(crate) mod search;

use alloc::vec::Vec;

use crate::builder::SolverBuilder;
use crate::error::{Error, Result};
use crate::result::{Limited, Model, Solution, Solutions, UnsatCore};
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
    clauses: Vec<Clause>,
    num_vars: u32,
    stats: Statistics,
    decision_level: DecisionLevel,
}

impl Solver {
    /// Creates a solver with default configuration.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            clauses: Vec::new(),
            num_vars: 0,
            stats: Statistics {
                decisions: 0,
                conflicts: 0,
                propagations: 0,
                restarts: 0,
                learned: 0,
                removed: 0,
                variables: 0,
                clauses: 0,
            },
            decision_level: DecisionLevel::GROUND,
        }
    }

    /// Returns a builder for configuring a new solver.
    #[inline]
    pub fn builder() -> SolverBuilder {
        SolverBuilder::new()
    }

    /// Allocates one fresh variable.
    pub fn new_var(&mut self) -> Result<Var> {
        if self.num_vars >= Var::MAX_RAW {
            return Err(Error::VariableLimitExceeded);
        }
        self.num_vars += 1;
        self.stats.variables = u64::from(self.num_vars);
        Var::new(self.num_vars).ok_or(Error::VariableLimitExceeded)
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
        self.clauses.push(Clause::from_lits(lits));
        #[allow(clippy::cast_possible_truncation)]
        {
            self.stats.clauses = self.clauses.len() as u64;
        }
    }

    /// Appends an already-built clause.
    pub fn add_clause(&mut self, clause: Clause) {
        self.clauses.push(clause);
        #[allow(clippy::cast_possible_truncation)]
        {
            self.stats.clauses = self.clauses.len() as u64;
        }
    }

    /// Returns the number of variables known to the solver.
    #[inline]
    #[must_use]
    pub const fn num_vars(&self) -> u32 {
        self.num_vars
    }

    /// Returns the number of clauses currently held by the solver.
    #[inline]
    #[must_use]
    pub fn num_clauses(&self) -> usize {
        self.clauses.len()
    }

    /// Returns the solver's accumulated statistics.
    #[inline]
    #[must_use]
    pub const fn statistics(&self) -> Statistics {
        self.stats
    }

    /// Returns the solver's current decision level.
    #[inline]
    #[must_use]
    pub const fn decision_level(&self) -> DecisionLevel {
        self.decision_level
    }

    /// Returns the current three-valued truth value of the given literal.
    #[must_use]
    pub const fn value(&self, _lit: Lit) -> Value {
        Value::Unassigned
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
    pub fn solve(&mut self) -> Result<Solution<'_>> {
        Err(Error::NotImplemented)
    }

    /// Drives search under the given assumption literals.
    pub fn solve_under<I: IntoIterator<Item = Lit>>(&mut self, _assumptions: I) -> Result<Limited<'_>> {
        Err(Error::NotImplemented)
    }

    /// Returns the model for the most recent SAT result, if any.
    #[must_use]
    pub const fn model(&self) -> Option<Model<'_>> {
        let _ = self;
        None
    }

    /// Returns the UNSAT core for the most recent UNSAT result, if any.
    #[must_use]
    pub const fn unsat_core(&self) -> Option<UnsatCore<'_>> {
        let _ = self;
        None
    }

    /// Returns an iterator over every satisfying assignment.
    pub fn solutions(&mut self) -> Solutions<'_> {
        Solutions::new(self)
    }
}

#[cfg(test)]
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
    fn solve_returns_not_implemented() {
        let mut s = Solver::new();
        assert_eq!(s.solve().err(), Some(Error::NotImplemented));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn new_var_bumps_count() {
        let mut s = Solver::new();
        let _ = s.new_var().unwrap();
        let _ = s.new_var().unwrap();
        assert_eq!(s.num_vars(), 2);
        assert_eq!(s.statistics().variables, 2);
    }
}
