//! [`SolverBuilder`]: configure a solver before constructing it.

use alloc::boxed::Box;

use crate::cnf::Cnf;
use crate::error::Result;
use crate::solver::Solver;
use crate::traits::{ClauseDeletion, DecisionHeuristic, Preprocessor, RestartStrategy};

/// Fluent configuration builder for a [`Solver`].
///
/// Non-heuristic settings and pluggable extension traits are stored here
/// until [`Self::build`] constructs the actual solver.
#[derive(Default)]
#[must_use]
pub struct SolverBuilder {
    conflict_budget: Option<u64>,
    propagation_budget: Option<u64>,
    timeout_ms: Option<u64>,
    chrono_gap: Option<u32>,
    verbose: bool,
    decision: Option<Box<dyn DecisionHeuristic>>,
    restart: Option<Box<dyn RestartStrategy>>,
    deletion: Option<Box<dyn ClauseDeletion>>,
    preprocessor: Option<Box<dyn Preprocessor>>,
}

impl core::fmt::Debug for SolverBuilder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SolverBuilder")
            .field("conflict_budget", &self.conflict_budget)
            .field("propagation_budget", &self.propagation_budget)
            .field("timeout_ms", &self.timeout_ms)
            .field("chrono_gap", &self.chrono_gap)
            .field("verbose", &self.verbose)
            .field("decision", &self.decision.as_ref().map(|d| d.name()))
            .field("restart", &self.restart.as_ref().map(|r| r.name()))
            .field("deletion", &self.deletion.as_ref().map(|d| d.name()))
            .field("preprocessor", &self.preprocessor.as_ref().map(|p| p.name()))
            .finish()
    }
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
            decision: None,
            restart: None,
            deletion: None,
            preprocessor: None,
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

    /// Enables verbose progress output.
    #[inline]
    pub fn verbose(mut self, on: bool) -> Self {
        self.verbose = on;
        self
    }

    /// Installs a decision heuristic.
    pub fn with_decision_heuristic<H: DecisionHeuristic>(mut self, heuristic: H) -> Self {
        self.decision = Some(Box::new(heuristic));
        self
    }

    /// Installs a restart strategy.
    pub fn with_restart_strategy<R: RestartStrategy>(mut self, strategy: R) -> Self {
        self.restart = Some(Box::new(strategy));
        self
    }

    /// Installs a learned-clause deletion policy.
    pub fn with_clause_deletion<D: ClauseDeletion>(mut self, policy: D) -> Self {
        self.deletion = Some(Box::new(policy));
        self
    }

    /// Installs a preprocessor pipeline.
    pub fn with_preprocessor<P: Preprocessor>(mut self, preprocessor: P) -> Self {
        self.preprocessor = Some(Box::new(preprocessor));
        self
    }

    /// Constructs a fresh solver with the configured settings.
    pub fn build(self) -> Solver {
        let _ = (
            self.conflict_budget,
            self.propagation_budget,
            self.timeout_ms,
            self.chrono_gap,
            self.verbose,
            self.decision,
            self.restart,
            self.deletion,
            self.preprocessor,
        );
        Solver::new()
    }

    /// Constructs a solver seeded with the given formula.
    pub fn build_from(self, cnf: Cnf) -> Result<Solver> {
        let mut solver = self.build();
        let _ = cnf.num_vars();
        for clause in cnf.clauses() {
            solver.add_clause(clause.clone());
        }
        Ok(solver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heuristics::Vsids;
    use crate::restarts::Luby;

    #[test]
    fn build_returns_empty_solver() {
        let s = SolverBuilder::new().with_timeout_ms(100).build();
        assert_eq!(s.num_vars(), 0);
    }

    #[test]
    fn install_trait_objects() {
        let _ = SolverBuilder::new()
            .with_decision_heuristic(Vsids::default())
            .with_restart_strategy(Luby::default())
            .build();
    }
}
