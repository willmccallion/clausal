//! Failed-literal probing.
//!
//! At the ground decision level, scans the top-`MAX_VARS` unassigned
//! variables by VSIDS activity. For each candidate, assumes one polarity at
//! a throwaway decision level and propagates; if that derives a conflict,
//! the opposite polarity is implied and gets installed as a root-level
//! unit. A follow-up propagation catches any cascade the new unit triggers.
//!
//! Budget is a fraction of the propagations recorded since the previous
//! probe pass, so the pass never dominates an otherwise-healthy search.

use alloc::vec::Vec;

use crate::internal::arena::ClauseArena;
use crate::internal::reason::Reason;
use crate::internal::trail::Assignment;
use crate::internal::watcher::{BinaryWatchers, LongWatchers};
use crate::solver::inprocess::InprocessOutcome;
use crate::solver::search::backtrack::backtrack_to;
use crate::solver::search::propagate::propagate;
use crate::types::{DecisionLevel, Value, Var};

/// Upper bound on the number of candidate variables probed per pass.
pub(crate) const MAX_VARS: usize = 1024;

/// Runs a failed-literal probing pass at the ground level.
///
/// `num_vars` is the solver's active variable count (one-based). `budget`
/// caps the number of propagation steps this pass may consume; zero
/// disables the cap entirely.
pub(crate) fn probe(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &mut BinaryWatchers,
    activities: &[f64],
    num_vars: u32,
    budget: u64,
) -> InprocessOutcome {
    debug_assert!(assignment.current_level().is_ground());
    if num_vars == 0 {
        return InprocessOutcome::Continue;
    }

    let mut candidates: Vec<Var> = Vec::with_capacity(num_vars as usize);
    for n in 1..=num_vars {
        let Some(var) = Var::new(n) else { continue };
        if assignment.value(var) == Value::Unassigned {
            candidates.push(var);
        }
    }
    candidates.sort_by(|a, b| {
        let av = activities.get(a.index()).copied().unwrap_or(0.0);
        let bv = activities.get(b.index()).copied().unwrap_or(0.0);
        bv.partial_cmp(&av).unwrap_or(core::cmp::Ordering::Equal)
    });
    let k = core::cmp::min(candidates.len(), MAX_VARS);

    let mut propagations: u64 = 0;
    let mut probed: usize = 0;
    while probed < k {
        if budget > 0 && propagations > budget {
            break;
        }
        let var = candidates[probed];
        probed += 1;

        for iter in 0u32..2 {
            if assignment.value(var) != Value::Unassigned {
                break;
            }
            let first_positive = assignment.saved_phase(var);
            let positive = if iter == 0 { first_positive } else { !first_positive };
            let probe_lit = if positive { var.pos() } else { var.neg() };

            assignment.push_decision_level();
            let lvl = assignment.current_level();
            assignment.assign(probe_lit, Reason::decision(), lvl);
            let before = assignment.trail_len();
            let conflict = propagate(arena, assignment, long_watchers, bin_watchers);
            let after = assignment.trail_len();
            propagations = propagations.saturating_add((after - before) as u64);
            backtrack_to(assignment, DecisionLevel::GROUND);

            if conflict.is_some() {
                let unit = !probe_lit;
                match assignment.value_of(unit) {
                    Value::True => {}
                    Value::False => return InprocessOutcome::Unsat,
                    Value::Unassigned => {
                        assignment.assign(unit, Reason::decision(), DecisionLevel::GROUND);
                    }
                }
                let before = assignment.trail_len();
                let cascade_conflict =
                    propagate(arena, assignment, long_watchers, bin_watchers);
                let after = assignment.trail_len();
                propagations = propagations.saturating_add((after - before) as u64);
                if cascade_conflict.is_some() {
                    return InprocessOutcome::Unsat;
                }
            }
        }
    }

    InprocessOutcome::Continue
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::internal::watcher::{
        attach_binary, ensure_binary_size, ensure_long_size, BinaryWatchers, LongWatchers,
    };
    use crate::types::{Lit, Var};

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    struct Harness {
        arena: ClauseArena,
        assignment: Assignment,
        lw: LongWatchers,
        bw: BinaryWatchers,
        activities: Vec<f64>,
        num_vars: u32,
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
            let activities = alloc::vec![0.0; num_vars as usize];
            Self {
                arena: ClauseArena::new(),
                assignment,
                lw,
                bw,
                activities,
                num_vars,
            }
        }

        fn add_binary(&mut self, a: Lit, b: Lit) {
            attach_binary(&mut self.bw, [a, b]);
        }

        fn run(&mut self) -> InprocessOutcome {
            probe(
                &mut self.arena,
                &mut self.assignment,
                &mut self.lw,
                &mut self.bw,
                &self.activities,
                self.num_vars,
                0,
            )
        }
    }

    #[test]
    fn empty_problem_continues() {
        let mut h = Harness::new(0);
        assert_eq!(h.run(), InprocessOutcome::Continue);
    }

    #[test]
    fn no_failure_leaves_trail_clean() {
        let mut h = Harness::new(2);
        h.add_binary(v(1).pos(), v(2).pos());
        assert_eq!(h.run(), InprocessOutcome::Continue);
        assert_eq!(h.assignment.value(v(1)), Value::Unassigned);
        assert_eq!(h.assignment.value(v(2)), Value::Unassigned);
    }

    #[test]
    fn probe_derives_forced_unit() {
        // (!x1 v !x2) and (!x1 v x2). Probing x1=true contradicts; x1=false
        // must be derived as a unit.
        let mut h = Harness::new(2);
        h.add_binary(v(1).neg(), v(2).neg());
        h.add_binary(v(1).neg(), v(2).pos());
        h.assignment.set_saved_phase(v(1), true);
        assert_eq!(h.run(), InprocessOutcome::Continue);
        assert_eq!(h.assignment.value(v(1)), Value::False);
    }

    #[test]
    fn probe_detects_trivial_unsat() {
        // (!x1 v !x2), (!x1 v x2), (x1 v !x2), (x1 v x2). Both polarities
        // of x1 conflict; probing derives UNSAT on the follow-up cascade.
        let mut h = Harness::new(2);
        h.add_binary(v(1).neg(), v(2).neg());
        h.add_binary(v(1).neg(), v(2).pos());
        h.add_binary(v(1).pos(), v(2).neg());
        h.add_binary(v(1).pos(), v(2).pos());
        assert_eq!(h.run(), InprocessOutcome::Unsat);
    }
}
