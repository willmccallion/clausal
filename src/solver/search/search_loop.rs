//! The CDCL search loop.
//!
//! Ties propagation, conflict analysis, backtracking, and VSIDS branching
//! together. On conflict the loop runs 1-UIP analysis (which bumps VSIDS
//! activities along the implication graph), installs the learned clause,
//! backtracks to the asserting level, and assigns the asserting literal.
//! Between conflicts the variable with the highest activity is popped
//! from the heap and branched on, polarity chosen by phase saving.

use alloc::vec::Vec;

use crate::internal::arena::ClauseArena;
use crate::internal::reason::Reason;
use crate::internal::trail::Assignment;
use crate::internal::watcher::{attach_binary, attach_long, BinaryWatchers, LongWatchers};
use crate::solver::order_heap::OrderHeap;
use crate::solver::reduce::{compact, reduce_learned, ReduceState};
use crate::solver::rephase::RephaseState;
use crate::solver::restart::RestartState;
use crate::solver::search::analyze::analyze;
use crate::solver::search::backtrack::backtrack_to;
use crate::solver::search::propagate::propagate;
use crate::types::{ClauseId, DecisionLevel, Lit, Var};

/// VSIDS variable decay. Each conflict divides `var_inc` by this factor,
/// so future bumps weigh more than past ones.
const VAR_DECAY: f64 = 0.95;

/// Mutable state reused across iterations of the CDCL main loop.
///
/// Owning every `Vec` across iterations keeps the hot loop allocation-free.
/// The heap and activity array together implement VSIDS branching; the
/// analyze-scratch slots feed the 1-UIP walk.
#[derive(Debug, Default)]
pub(crate) struct SearchScratch {
    pub(crate) seen: Vec<bool>,
    pub(crate) learned: Vec<Lit>,
    pub(crate) stack: Vec<Var>,
    pub(crate) to_clear: Vec<Var>,
    pub(crate) levels: Vec<DecisionLevel>,
    pub(crate) heap: OrderHeap,
    pub(crate) activities: Vec<f64>,
    pub(crate) var_inc: f64,
    pub(crate) restart: RestartState,
    pub(crate) restarts: u64,
    pub(crate) conflicts: u64,
    pub(crate) reduce: ReduceState,
    pub(crate) reductions: u64,
    pub(crate) compactions: u64,
    pub(crate) rephase: RephaseState,
    pub(crate) rephases: u64,
}

impl SearchScratch {
    /// Creates an empty scratch. `var_inc` starts at `1.0`.
    pub(crate) const fn new() -> Self {
        Self {
            seen: Vec::new(),
            learned: Vec::new(),
            stack: Vec::new(),
            to_clear: Vec::new(),
            levels: Vec::new(),
            heap: OrderHeap::new(),
            activities: Vec::new(),
            var_inc: 1.0,
            restart: RestartState::new(),
            restarts: 0,
            conflicts: 0,
            reduce: ReduceState::new(),
            reductions: 0,
            compactions: 0,
            rephase: RephaseState::new(),
            rephases: 0,
        }
    }

    /// Ensures internal storage is sized for `num_vars` variables and seeds
    /// the heap with every variable.
    pub(crate) fn grow_to(&mut self, num_vars: usize) {
        if self.activities.len() < num_vars {
            self.activities.resize(num_vars, 0.0);
        }
        self.heap.grow_to(num_vars);
        #[allow(clippy::cast_possible_truncation)]
        for i in 0..num_vars {
            let raw = (i as u32).saturating_add(1);
            let Some(var) = Var::new(raw) else { continue };
            self.heap.insert(var, &self.activities);
        }
    }
}

/// Internal verdict produced by the search loop.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SearchOutcome {
    /// The formula is satisfiable; the current assignment is a model.
    Sat,
    /// The formula is unsatisfiable.
    Unsat,
}

/// Runs the CDCL search loop until SAT or UNSAT is determined.
pub(crate) fn solve_loop(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &mut BinaryWatchers,
    learned_clauses: &mut Vec<ClauseId>,
    scratch: &mut SearchScratch,
) -> SearchOutcome {
    loop {
        if let Some(conflict) = propagate(arena, assignment, long_watchers, bin_watchers) {
            let conflict_level = conflict.level_of(arena, assignment);
            if conflict_level.is_ground() {
                return SearchOutcome::Unsat;
            }
            scratch.conflicts = scratch.conflicts.saturating_add(1);
            let peak_trail_len = assignment.trail().len();
            let (backjump, lbd) = analyze(
                arena,
                assignment,
                conflict,
                conflict_level,
                &mut scratch.seen,
                &mut scratch.learned,
                &mut scratch.stack,
                &mut scratch.to_clear,
                &mut scratch.levels,
                &mut scratch.activities,
                &mut scratch.var_inc,
            );
            if scratch.learned.is_empty() {
                return SearchOutcome::Unsat;
            }
            // Activity bumps in analyze may have moved literals upward in
            // the heap; push them to their new heights.
            for &var in &scratch.to_clear {
                scratch.heap.update_bumped(var, &scratch.activities);
            }
            scratch.var_inc /= VAR_DECAY;
            #[allow(clippy::cast_precision_loss)]
            scratch
                .restart
                .record_conflict(f64::from(lbd), peak_trail_len as f64);
            reinsert_from_trail(assignment, &mut scratch.heap, &scratch.activities, backjump);
            backtrack_to(assignment, backjump);
            install_learned(
                arena,
                assignment,
                long_watchers,
                bin_watchers,
                learned_clauses,
                &scratch.learned,
                backjump,
            );
            #[allow(clippy::cast_precision_loss)]
            let restart = scratch.restart.should_restart(peak_trail_len as f64);
            let reduce = scratch.reduce.should_reduce(scratch.conflicts);
            let rephase = scratch.rephase.should_rephase(scratch.conflicts);
            if restart || reduce || rephase {
                reinsert_from_trail(
                    assignment,
                    &mut scratch.heap,
                    &scratch.activities,
                    DecisionLevel::GROUND,
                );
                backtrack_to(assignment, DecisionLevel::GROUND);
                if restart {
                    scratch.restart.reset_window();
                    scratch.restarts = scratch.restarts.saturating_add(1);
                }
                if reduce {
                    reduce_learned(arena, long_watchers, learned_clauses, assignment);
                    scratch.reduce.on_reduced();
                    scratch.reductions = scratch.reductions.saturating_add(1);
                    if scratch.reduce.should_compact()
                        && compact(arena, long_watchers, learned_clauses, assignment).is_ok()
                    {
                        scratch.compactions = scratch.compactions.saturating_add(1);
                    }
                }
                if rephase {
                    scratch.rephase.apply(assignment, scratch.conflicts);
                    scratch.rephases = scratch.rephases.saturating_add(1);
                }
            }
        } else if let Some(var) = pick_branching_var(&mut scratch.heap, &scratch.activities, assignment) {
            let lit = if assignment.saved_phase(var) { var.pos() } else { var.neg() };
            assignment.push_decision_level();
            let lvl = assignment.current_level();
            assignment.assign(lit, Reason::decision(), lvl);
        } else {
            return SearchOutcome::Sat;
        }
    }
}

/// Installs the learned clause at the appropriate size and assigns its
/// asserting literal.
fn install_learned(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &mut BinaryWatchers,
    learned_clauses: &mut Vec<ClauseId>,
    learned: &[Lit],
    backjump: DecisionLevel,
) {
    match learned.len() {
        0 => {}
        1 => {
            let asserting = learned[0];
            assignment.assign(asserting, Reason::decision(), DecisionLevel::GROUND);
        }
        2 => {
            let asserting = learned[0];
            let partner = learned[1];
            attach_binary(bin_watchers, [asserting, partner]);
            assignment.assign(asserting, Reason::binary(partner), backjump);
        }
        _ => {
            let Ok(id) = arena.push(learned, true, 0) else {
                return;
            };
            attach_long(long_watchers, arena, id);
            learned_clauses.push(id);
            let asserting = learned[0];
            assignment.assign(asserting, Reason::long(id), backjump);
        }
    }
}

/// Pops the highest-activity unassigned variable. Skips any heap entries
/// that happen to be already assigned (stale after propagation installs
/// them implicitly).
fn pick_branching_var(
    heap: &mut OrderHeap,
    activities: &[f64],
    assignment: &Assignment,
) -> Option<Var> {
    while let Some(var) = heap.pop_max(activities) {
        if !assignment.is_assigned(var) {
            return Some(var);
        }
    }
    None
}

/// Re-inserts every variable about to be unassigned by a backtrack.
///
/// Literals at trail positions at or after `trail_lim[backjump]` are about
/// to lose their assignment. Whichever of those were decisions got popped
/// from the heap at branch time; propagated vars were never popped. In
/// both cases, `heap.insert` is idempotent, so iterating the trail tail
/// once covers both populations.
fn reinsert_from_trail(
    assignment: &Assignment,
    heap: &mut OrderHeap,
    activities: &[f64],
    backjump: DecisionLevel,
) {
    if assignment.current_level().get() <= backjump.get() {
        return;
    }
    let keep_from = assignment.trail_lim()[backjump.get() as usize] as usize;
    let trail = assignment.trail();
    for &lit in &trail[keep_from..] {
        heap.insert(lit.var(), activities);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::internal::watcher::{ensure_binary_size, ensure_long_size};
    use crate::types::Value;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    struct Harness {
        arena: ClauseArena,
        assignment: Assignment,
        lw: LongWatchers,
        bw: BinaryWatchers,
        learned: Vec<ClauseId>,
        scratch: SearchScratch,
    }

    impl Harness {
        fn new(num_vars: u32) -> Self {
            let mut assignment = Assignment::new();
            for n in 1..=num_vars {
                assignment.ensure_var(v(n));
            }
            let mut lw: LongWatchers = Vec::new();
            let mut bw: BinaryWatchers = Vec::new();
            ensure_long_size(&mut lw, num_vars as usize);
            ensure_binary_size(&mut bw, num_vars as usize);
            let mut scratch = SearchScratch::new();
            scratch.grow_to(num_vars as usize);
            Self {
                arena: ClauseArena::new(),
                assignment,
                lw,
                bw,
                learned: Vec::new(),
                scratch,
            }
        }

        fn add_binary(&mut self, a: Lit, b: Lit) {
            attach_binary(&mut self.bw, [a, b]);
        }

        fn add_long(&mut self, lits: &[Lit]) {
            let id = self.arena.push(lits, false, 0).unwrap();
            attach_long(&mut self.lw, &self.arena, id);
        }

        fn add_unit(&mut self, lit: Lit) {
            self.assignment.assign(lit, Reason::decision(), DecisionLevel::GROUND);
        }

        fn solve(&mut self) -> SearchOutcome {
            solve_loop(
                &mut self.arena,
                &mut self.assignment,
                &mut self.lw,
                &mut self.bw,
                &mut self.learned,
                &mut self.scratch,
            )
        }
    }

    #[test]
    fn empty_formula_is_sat() {
        let mut h = Harness::new(3);
        assert_eq!(h.solve(), SearchOutcome::Sat);
    }

    #[test]
    fn single_unit_is_sat() {
        let mut h = Harness::new(2);
        h.add_unit(v(1).pos());
        assert_eq!(h.solve(), SearchOutcome::Sat);
        assert_eq!(h.assignment.value(v(1)), Value::True);
    }

    #[test]
    fn contradictory_units_is_unsat() {
        let mut h = Harness::new(1);
        h.add_binary(v(1).pos(), v(1).pos());
        h.add_binary(v(1).neg(), v(1).neg());
        assert_eq!(h.solve(), SearchOutcome::Unsat);
    }

    #[test]
    fn binary_forced_cascade_is_sat() {
        let mut h = Harness::new(3);
        h.add_binary(v(1).neg(), v(2).pos());
        h.add_binary(v(2).neg(), v(3).pos());
        h.add_unit(v(1).pos());
        assert_eq!(h.solve(), SearchOutcome::Sat);
        assert_eq!(h.assignment.value(v(1)), Value::True);
        assert_eq!(h.assignment.value(v(2)), Value::True);
        assert_eq!(h.assignment.value(v(3)), Value::True);
    }

    #[test]
    fn three_sat_requires_branching() {
        let mut h = Harness::new(3);
        h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        assert_eq!(h.solve(), SearchOutcome::Sat);
        let ok = h.assignment.value(v(1)) == Value::True
            || h.assignment.value(v(2)) == Value::True
            || h.assignment.value(v(3)) == Value::True;
        assert!(ok, "clause must be satisfied under the final model");
    }

    #[test]
    fn pigeonhole_like_unsat() {
        let mut h = Harness::new(2);
        h.add_unit(v(1).pos());
        h.add_unit(v(2).pos());
        h.add_binary(v(1).neg(), v(2).neg());
        assert_eq!(h.solve(), SearchOutcome::Unsat);
    }

    #[test]
    fn conflict_then_learn_then_satisfy() {
        let mut h = Harness::new(3);
        h.add_binary(v(1).neg(), v(2).pos());
        h.add_binary(v(1).neg(), v(2).neg());
        h.add_binary(v(3).neg(), v(1).pos());
        assert_eq!(h.solve(), SearchOutcome::Sat);
        assert_eq!(h.assignment.value(v(1)), Value::False);
        assert_eq!(h.assignment.value(v(3)), Value::False);
    }

    #[test]
    fn long_clause_conflict_drives_learning() {
        let mut h = Harness::new(3);
        h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        h.add_long(&[v(1).neg(), v(2).neg(), v(3).neg()]);
        h.add_unit(v(1).pos());
        h.add_unit(v(2).pos());
        h.add_unit(v(3).pos());
        assert_eq!(h.solve(), SearchOutcome::Unsat);
    }

    #[test]
    fn hard_instance_fires_restarts() {
        // Pigeonhole PHP(6, 5): 6 pigeons in 5 holes is UNSAT. The solver
        // should chew through enough conflicts to trigger at least one
        // Glucose restart before proving UNSAT.
        let pigeons = 6u32;
        let holes = 5u32;
        let var_of = |p: u32, h: u32| -> Var {
            let n = (p - 1) * holes + h;
            v(n)
        };
        let num_vars = pigeons * holes;
        let mut h = Harness::new(num_vars);
        // Every pigeon goes somewhere.
        for p in 1..=pigeons {
            let clause: Vec<Lit> = (1..=holes).map(|hi| var_of(p, hi).pos()).collect();
            h.add_long(&clause);
        }
        // No two pigeons share a hole.
        for hi in 1..=holes {
            for p1 in 1..=pigeons {
                for p2 in (p1 + 1)..=pigeons {
                    h.add_binary(var_of(p1, hi).neg(), var_of(p2, hi).neg());
                }
            }
        }
        assert_eq!(h.solve(), SearchOutcome::Unsat);
        assert!(
            h.scratch.restarts > 0,
            "expected at least one restart (conflicts={})",
            h.scratch.conflicts,
        );
    }

    #[test]
    fn vsids_bumps_touched_variables() {
        // With default phase=false, deciding x1=false propagates x2=true
        // and x3=false via the first, second, and fourth clauses. The
        // third clause (!x2 v x3) then conflicts. Analyze must bump the
        // activities of variables it walks through.
        let mut h = Harness::new(3);
        h.add_binary(v(1).pos(), v(2).pos());
        h.add_binary(v(1).neg(), v(3).pos());
        h.add_binary(v(2).neg(), v(3).pos());
        h.add_binary(v(1).pos(), v(3).neg());
        h.add_binary(v(1).neg(), v(3).neg());
        let _ = h.solve();
        assert!(
            h.scratch.activities[v(1).index()] > 0.0
                || h.scratch.activities[v(2).index()] > 0.0
                || h.scratch.activities[v(3).index()] > 0.0,
            "at least one touched var should have nonzero activity"
        );
    }
}
