//! End-to-end engine state driven by [`crate::Solver`].
//!
//! Owns every subsystem the CDCL loop needs: the clause arena, the
//! assignment and trail, the long- and binary-clause watch tables, the
//! learned-clause registry, the branching/restart/reduce/rephase scratch,
//! and the last-outcome cache queried by `Solver::model`. Installing a
//! user-supplied clause here canonicalizes it for the engine: sort out
//! tautologies, deduplicate literals, detect ground-level unit conflicts,
//! and route unit/binary/long clauses into the correct storage.

use alloc::vec::Vec;

use crate::internal::arena::ClauseArena;
use crate::internal::reason::Reason;
use crate::internal::trail::Assignment;
use crate::internal::watcher::{
    attach_binary, attach_long, ensure_binary_size, ensure_long_size, BinaryWatchers,
    LongWatchers,
};
use crate::result::InterruptReason;
use crate::solver::search::search_loop::{solve_loop, SearchOutcome, SearchScratch};
use crate::types::{ClauseId, DecisionLevel, Lit, Value, Var};

/// Records whether the most recent `solve` returned SAT or UNSAT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LastOutcome {
    /// No `solve` has been run yet.
    None,
    /// The formula is satisfiable under the current assignment.
    Sat,
    /// The formula is unsatisfiable.
    Unsat,
}

/// Engine state shared across every solve call.
#[derive(Debug)]
pub(crate) struct SolverState {
    pub(crate) arena: ClauseArena,
    pub(crate) assignment: Assignment,
    pub(crate) long_watchers: LongWatchers,
    pub(crate) bin_watchers: BinaryWatchers,
    pub(crate) learned_clauses: Vec<ClauseId>,
    pub(crate) scratch: SearchScratch,
    pub(crate) num_vars: u32,
    pub(crate) unsat_at_init: bool,
    pub(crate) last_outcome: LastOutcome,
    pub(crate) last_core: Vec<Lit>,
    #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))]
    pub(crate) interrupter: Option<crate::interrupter::Interrupter>,
}

impl Default for SolverState {
    fn default() -> Self {
        Self::new()
    }
}

impl SolverState {
    /// Creates a fresh, empty engine state.
    pub(crate) const fn new() -> Self {
        Self {
            arena: ClauseArena::new(),
            assignment: Assignment::new(),
            long_watchers: Vec::new(),
            bin_watchers: Vec::new(),
            learned_clauses: Vec::new(),
            scratch: SearchScratch::new(),
            num_vars: 0,
            unsat_at_init: false,
            last_outcome: LastOutcome::None,
            last_core: Vec::new(),
            #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))]
            interrupter: None,
        }
    }

    /// Grows internal storage so every structure addresses at least
    /// `num_vars` variables. Safe to call repeatedly.
    pub(crate) fn grow_to(&mut self, num_vars: u32) {
        if num_vars <= self.num_vars {
            return;
        }
        self.num_vars = num_vars;
        for n in 1..=num_vars {
            if let Some(v) = Var::new(n) {
                self.assignment.ensure_var(v);
            }
        }
        ensure_long_size(&mut self.long_watchers, num_vars as usize);
        ensure_binary_size(&mut self.bin_watchers, num_vars as usize);
        self.scratch.grow_to(num_vars as usize);
    }

    /// Installs a user-supplied clause into the engine.
    ///
    /// Skips tautologies and deduplicates literals. A clause that is already
    /// satisfied at the ground level is silently ignored; a conflicting
    /// unit or an empty clause marks the formula as unsatisfiable at init.
    pub(crate) fn install_user_clause(&mut self, lits: &[Lit]) {
        if self.unsat_at_init {
            return;
        }
        let mut buf: Vec<Lit> = Vec::with_capacity(lits.len());
        for &lit in lits {
            let v = lit.var();
            if v.to_raw() > self.num_vars {
                self.grow_to(v.to_raw());
            }
            if self.assignment.value_of(lit) == Value::True
                && self.assignment.level(lit.var()).is_ground()
            {
                return;
            }
            if self.assignment.value_of(lit) == Value::False
                && self.assignment.level(lit.var()).is_ground()
            {
                continue;
            }
            if buf.contains(&!lit) {
                return;
            }
            if !buf.contains(&lit) {
                buf.push(lit);
            }
        }
        match buf.as_slice() {
            [] => {
                self.unsat_at_init = true;
            }
            [unit] => {
                self.assign_unit(*unit);
            }
            [a, b] => {
                attach_binary(&mut self.bin_watchers, [*a, *b]);
            }
            _ => {
                if let Ok(id) = self.arena.push(&buf, false, 0) {
                    attach_long(&mut self.long_watchers, &self.arena, id);
                }
            }
        }
    }

    fn assign_unit(&mut self, unit: Lit) {
        match self.assignment.value_of(unit) {
            Value::True => {}
            Value::False => {
                self.unsat_at_init = true;
            }
            Value::Unassigned => {
                self.assignment.assign(unit, Reason::decision(), DecisionLevel::GROUND);
            }
        }
    }

    /// Runs the CDCL main loop to a verdict.
    pub(crate) fn solve(&mut self) -> SearchOutcome {
        self.last_core.clear();
        if self.unsat_at_init {
            self.last_outcome = LastOutcome::Unsat;
            return SearchOutcome::Unsat;
        }
        let outcome = solve_loop(
            &mut self.arena,
            &mut self.assignment,
            &mut self.long_watchers,
            &mut self.bin_watchers,
            &mut self.learned_clauses,
            &mut self.scratch,
            &[],
            &mut self.last_core,
            || false,
        );
        self.last_outcome = match outcome {
            SearchOutcome::Sat => LastOutcome::Sat,
            SearchOutcome::Unsat => LastOutcome::Unsat,
            SearchOutcome::Interrupted => LastOutcome::None,
        };
        outcome
    }

    /// Runs the CDCL main loop with a set of assumption literals and an
    /// optional cooperative abort check.
    pub(crate) fn solve_under(
        &mut self,
        assumptions: &[Lit],
    ) -> (SearchOutcome, Option<InterruptReason>) {
        self.last_core.clear();
        for &lit in assumptions {
            if lit.var().to_raw() > self.num_vars {
                self.grow_to(lit.var().to_raw());
            }
        }
        if self.unsat_at_init {
            self.last_outcome = LastOutcome::Unsat;
            return (SearchOutcome::Unsat, None);
        }
        let outcome = self.run_with_assumptions(assumptions);
        match outcome {
            SearchOutcome::Sat => {
                self.last_outcome = LastOutcome::Sat;
                (outcome, None)
            }
            SearchOutcome::Unsat => {
                self.last_outcome = LastOutcome::Unsat;
                (outcome, None)
            }
            SearchOutcome::Interrupted => {
                self.last_outcome = LastOutcome::None;
                (outcome, Some(InterruptReason::External))
            }
        }
    }

    #[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))]
    fn run_with_assumptions(&mut self, assumptions: &[Lit]) -> SearchOutcome {
        let interrupter = self.interrupter.clone();
        let abort = move || {
            interrupter
                .as_ref()
                .is_some_and(crate::interrupter::Interrupter::is_interrupted)
        };
        solve_loop(
            &mut self.arena,
            &mut self.assignment,
            &mut self.long_watchers,
            &mut self.bin_watchers,
            &mut self.learned_clauses,
            &mut self.scratch,
            assumptions,
            &mut self.last_core,
            abort,
        )
    }

    #[cfg(not(all(target_has_atomic = "8", target_has_atomic = "ptr")))]
    fn run_with_assumptions(&mut self, assumptions: &[Lit]) -> SearchOutcome {
        solve_loop(
            &mut self.arena,
            &mut self.assignment,
            &mut self.long_watchers,
            &mut self.bin_watchers,
            &mut self.learned_clauses,
            &mut self.scratch,
            assumptions,
            &mut self.last_core,
            || false,
        )
    }

    /// Pops the trail back to the ground level and reseeds the VSIDS heap
    /// with every unassigned variable so search can restart cleanly after
    /// a previous SAT verdict.
    pub(crate) fn reset_search_state(&mut self) {
        self.assignment.pop_to(DecisionLevel::GROUND);
        for i in 0..self.assignment.num_vars() {
            #[allow(clippy::cast_possible_truncation)]
            let raw = (i as u32).saturating_add(1);
            let Some(var) = Var::new(raw) else { continue };
            if !self.assignment.is_assigned(var) {
                self.scratch.heap.insert(var, &self.scratch.activities);
            }
        }
    }

    /// Returns the truth value of `var` under the current assignment.
    pub(crate) fn value(&self, var: Var) -> Value {
        if var.to_raw() > self.num_vars {
            return Value::Unassigned;
        }
        self.assignment.value(var)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    #[test]
    fn empty_state_solves_sat() {
        let mut s = SolverState::new();
        assert_eq!(s.solve(), SearchOutcome::Sat);
    }

    #[test]
    fn ground_level_unit_conflict_is_unsat() {
        let mut s = SolverState::new();
        s.grow_to(1);
        s.install_user_clause(&[v(1).pos()]);
        s.install_user_clause(&[v(1).neg()]);
        assert!(s.unsat_at_init);
        assert_eq!(s.solve(), SearchOutcome::Unsat);
    }

    #[test]
    fn tautology_is_ignored() {
        let mut s = SolverState::new();
        s.install_user_clause(&[v(1).pos(), v(1).neg()]);
        assert_eq!(s.arena.num_clauses(), 0);
        assert!(!s.unsat_at_init);
    }

    #[test]
    fn duplicate_literal_is_dropped() {
        let mut s = SolverState::new();
        s.install_user_clause(&[v(1).pos(), v(1).pos(), v(2).pos()]);
        assert_eq!(s.solve(), SearchOutcome::Sat);
    }

    #[test]
    fn long_clause_is_installed() {
        let mut s = SolverState::new();
        s.install_user_clause(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        assert_eq!(s.arena.num_clauses(), 1);
    }

    #[test]
    fn binary_clause_is_satisfiable() {
        let mut s = SolverState::new();
        s.install_user_clause(&[v(1).pos(), v(2).neg()]);
        s.install_user_clause(&[v(1).neg(), v(2).pos()]);
        assert_eq!(s.solve(), SearchOutcome::Sat);
    }

    #[test]
    fn pigeonhole_like_unsat() {
        let mut s = SolverState::new();
        s.install_user_clause(&[v(1).pos()]);
        s.install_user_clause(&[v(2).pos()]);
        s.install_user_clause(&[v(1).neg(), v(2).neg()]);
        assert_eq!(s.solve(), SearchOutcome::Unsat);
    }

    #[test]
    fn satisfied_unit_drops_clause_without_marking_unsat() {
        let mut s = SolverState::new();
        s.install_user_clause(&[v(1).pos()]);
        // Already satisfied; second unit must not blow up.
        s.install_user_clause(&[v(1).pos(), v(2).neg()]);
        assert!(!s.unsat_at_init);
    }
}
