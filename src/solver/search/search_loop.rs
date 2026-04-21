//! The CDCL search loop.
//!
//! Ties propagation, conflict analysis, and backtracking together. On
//! conflict, the loop runs 1-UIP analysis, installs the learned clause,
//! backtracks to the asserting level, and assigns the asserting literal.
//! When no conflict surfaces, a branching literal is chosen and a fresh
//! decision level is opened.
//!
//! Stage 2 branches via a linear scan over variables keyed on their saved
//! phase. Stage 3 swaps in a VSIDS priority queue at the same call site.

use alloc::vec::Vec;

use crate::internal::arena::ClauseArena;
use crate::internal::reason::Reason;
use crate::internal::trail::Assignment;
use crate::internal::watcher::{attach_binary, attach_long, BinaryWatchers, LongWatchers};
use crate::solver::search::analyze::analyze;
use crate::solver::search::backtrack::backtrack_to;
use crate::solver::search::propagate::propagate;
use crate::types::{ClauseId, DecisionLevel, Lit, Value, Var};

/// Scratch buffers reused across conflict-analysis invocations.
///
/// Owning the backing `Vec`s across iterations keeps the CDCL hot loop
/// allocation-free.
#[derive(Debug, Default)]
pub(crate) struct AnalyzeScratch {
    pub(crate) seen: Vec<bool>,
    pub(crate) learned: Vec<Lit>,
    pub(crate) stack: Vec<Var>,
    pub(crate) to_clear: Vec<Var>,
    pub(crate) levels: Vec<DecisionLevel>,
}

impl AnalyzeScratch {
    pub(crate) const fn new() -> Self {
        Self {
            seen: Vec::new(),
            learned: Vec::new(),
            stack: Vec::new(),
            to_clear: Vec::new(),
            levels: Vec::new(),
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
    scratch: &mut AnalyzeScratch,
) -> SearchOutcome {
    loop {
        if let Some(conflict) = propagate(arena, assignment, long_watchers, bin_watchers) {
            let conflict_level = conflict.level_of(arena, assignment);
            if conflict_level.is_ground() {
                return SearchOutcome::Unsat;
            }
            let (backjump, _lbd) = analyze(
                arena,
                assignment,
                conflict,
                conflict_level,
                &mut scratch.seen,
                &mut scratch.learned,
                &mut scratch.stack,
                &mut scratch.to_clear,
                &mut scratch.levels,
            );
            if scratch.learned.is_empty() {
                return SearchOutcome::Unsat;
            }
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
        } else if let Some(var) = pick_branching_var(assignment) {
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

/// Linear scan for the lowest-index unassigned variable.
fn pick_branching_var(assignment: &Assignment) -> Option<Var> {
    let n = assignment.num_vars();
    for i in 0..n {
        #[allow(clippy::cast_possible_truncation)]
        let raw = (i as u32).saturating_add(1);
        let Some(var) = Var::new(raw) else { continue };
        if assignment.value(var) == Value::Unassigned {
            return Some(var);
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::internal::watcher::{ensure_binary_size, ensure_long_size};

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    struct Harness {
        arena: ClauseArena,
        assignment: Assignment,
        lw: LongWatchers,
        bw: BinaryWatchers,
        learned: Vec<ClauseId>,
        scratch: AnalyzeScratch,
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
            Self {
                arena: ClauseArena::new(),
                assignment,
                lw,
                bw,
                learned: Vec::new(),
                scratch: AnalyzeScratch::new(),
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
        // (x1). The trail pre-loaded at ground; propagate finds nothing and
        // the loop picks branches for the remaining vars.
        let mut h = Harness::new(2);
        h.add_unit(v(1).pos());
        assert_eq!(h.solve(), SearchOutcome::Sat);
        assert_eq!(h.assignment.value(v(1)), Value::True);
    }

    #[test]
    fn contradictory_units_is_unsat() {
        // (x1) and (!x1 v !x1) — encoded via direct binary conflict.
        let mut h = Harness::new(1);
        h.add_binary(v(1).pos(), v(1).pos());
        h.add_binary(v(1).neg(), v(1).neg());
        assert_eq!(h.solve(), SearchOutcome::Unsat);
    }

    #[test]
    fn binary_forced_cascade_is_sat() {
        // (!x1 v x2), (!x2 v x3). Force x1 true as a unit; propagation
        // cascades to x2=true and x3=true, leaving nothing to branch on.
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
        // (x1 v x2 v x3) — satisfiable but nothing forced. The loop must
        // branch to decide.
        let mut h = Harness::new(3);
        h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        assert_eq!(h.solve(), SearchOutcome::Sat);
        // At least one of x1, x2, x3 must be true under any satisfying model.
        let ok = h.assignment.value(v(1)) == Value::True
            || h.assignment.value(v(2)) == Value::True
            || h.assignment.value(v(3)) == Value::True;
        assert!(ok, "clause must be satisfied under the final model");
    }

    #[test]
    fn pigeonhole_like_unsat() {
        // Place two pigeons in one hole: variables x_ij mean pigeon i in
        // hole j. Two pigeons, one hole, forcing UNSAT via:
        //   (x11) and (x21) and (!x11 v !x21)  — both pigeons in hole 1,
        //   but not both.
        let mut h = Harness::new(2);
        h.add_unit(v(1).pos());
        h.add_unit(v(2).pos());
        h.add_binary(v(1).neg(), v(2).neg());
        assert_eq!(h.solve(), SearchOutcome::Unsat);
    }

    #[test]
    fn conflict_then_learn_then_satisfy() {
        // (!x1 v x2), (!x1 v !x2), (!x3 v x1). Branching picks x3+ first
        // (smallest var), then x1 gets implied by branching, then the two
        // x1-x2 clauses conflict, leading to a learned unit !x1 and then
        // !x3 at ground level.
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
        // Two long clauses that together force UNSAT:
        //   (x1 v x2 v x3) and (!x1 v !x2 v !x3) with unit constraints
        //   x1 true, x2 true, x3 true — the second long clause is
        //   falsified.
        let mut h = Harness::new(3);
        h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        h.add_long(&[v(1).neg(), v(2).neg(), v(3).neg()]);
        h.add_unit(v(1).pos());
        h.add_unit(v(2).pos());
        h.add_unit(v(3).pos());
        assert_eq!(h.solve(), SearchOutcome::Unsat);
    }
}
