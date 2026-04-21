//! [`SolverBuilder`]: configure a solver before constructing it.

use crate::cnf::Cnf;
use crate::error::Result;
use crate::solver::Solver;

/// Fluent configuration builder for a [`Solver`].
///
/// Non-heuristic settings are stored here until [`Self::build`] constructs
/// the actual solver. Heuristic, restart, clause-deletion, and preprocessor
/// slots are added in a later commit once the relevant traits exist.
#[derive(Default, Debug)]
#[must_use]
pub struct SolverBuilder {
    conflict_budget: Option<u64>,
    propagation_budget: Option<u64>,
    timeout_ms: Option<u64>,
    chrono_gap: Option<u32>,
    verbose: bool,
}

impl SolverBuilder {
    /// Creates a fresh builder with default settings.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            conflict_budget: None,
            propagation_budget: None,
            timeout_ms: None,
            chrono_gap: None,
            verbose: false,
        }
    }

    /// Sets a conflict budget. Search aborts after this many conflicts.
    #[inline]
    pub fn with_conflict_budget(mut self, budget: u64) -> Self {
        self.conflict_budget = Some(budget);
        self
    }

    /// Sets a propagation budget.
    #[inline]
    pub fn with_propagation_budget(mut self, budget: u64) -> Self {
        self.propagation_budget = Some(budget);
        self
    }

    /// Sets a wall-clock timeout in milliseconds.
    #[inline]
    pub fn with_timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    /// Sets the chronological-backtracking jump threshold.
    #[inline]
    pub fn with_chrono_gap(mut self, gap: u32) -> Self {
        self.chrono_gap = Some(gap);
        self
    }

    /// Enables verbose progress output (once a logger lands).
    #[inline]
    pub fn verbose(mut self, on: bool) -> Self {
        self.verbose = on;
        self
    }

    /// Constructs a fresh solver with the configured settings.
    ///
    /// Stub: currently ignores the configuration and returns a default
    /// solver. Configuration routing lands when the engine does.
    pub fn build(self) -> Solver {
        let _ = (
            self.conflict_budget,
            self.propagation_budget,
            self.timeout_ms,
            self.chrono_gap,
            self.verbose,
        );
        Solver::new()
    }

    /// Constructs a solver seeded with the given formula.
    ///
    /// Stub: currently appends every clause from the CNF and returns the
    /// bare solver.
    pub fn build_from(self, cnf: Cnf) -> Result<Solver> {
        let mut solver = self.build();
        let _ = cnf.num_vars();
        for _clause in cnf.clauses() {
            solver.add_clause(_clause.clone());
        }
        Ok(solver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_returns_empty_solver() {
        let s = SolverBuilder::new().with_timeout_ms(100).build();
        assert_eq!(s.num_vars(), 0);
    }
}
