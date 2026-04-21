//! Clause vivification.
//!
//! Walks low-LBD learned clauses at the ground level. For each candidate
//! clause, the pass attempts to shrink it: literals already falsified at
//! level 0 are dropped, literals already satisfied collapse the clause to
//! its earlier prefix, and a conflict derived from assuming the negation of
//! a prefix proves the prefix is already implied by the formula. The
//! original clause is then replaced by the (potentially much smaller) new
//! body, which may become a unit, a binary, or a shorter long clause.

use alloc::vec::Vec;

use crate::internal::arena::ClauseArena;
use crate::internal::reason::Reason;
use crate::internal::trail::Assignment;
use crate::internal::watcher::{
    attach_binary, attach_long, BinaryWatchers, LongWatchers,
};
use crate::solver::inprocess::InprocessOutcome;
use crate::solver::search::backtrack::backtrack_to;
use crate::solver::search::propagate::propagate;
use crate::types::{ClauseId, DecisionLevel, Lit, Value};

/// Learned clauses with LBD strictly above this cap are skipped.
pub(crate) const LBD_CAP: u32 = 6;

/// Runs a vivification pass at the ground decision level.
///
/// `budget` caps the number of propagation steps the pass may consume;
/// zero disables the cap entirely.
pub(crate) fn vivify(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &mut BinaryWatchers,
    learned_clauses: &mut Vec<ClauseId>,
    budget: u64,
) -> InprocessOutcome {
    debug_assert!(assignment.current_level().is_ground());

    let candidates: Vec<ClauseId> = learned_clauses
        .iter()
        .copied()
        .filter(|&id| !arena.is_deleted(id) && arena.lbd(id) <= LBD_CAP)
        .collect();

    let mut scratch: Vec<Lit> = Vec::new();
    let mut propagations: u64 = 0;
    let mut any_deleted = false;

    for cref in candidates {
        if budget > 0 && propagations > budget {
            break;
        }
        if arena.is_deleted(cref) {
            continue;
        }
        if is_locked_at_ground(arena, assignment, cref) {
            continue;
        }

        // Snapshot the clause body before doing anything that might
        // reallocate the arena.
        scratch.clear();
        scratch.extend_from_slice(arena.lits(cref));

        let mut shrink_len: Option<usize> = None;
        let mut subsumed = false;
        let mut i: usize = 0;
        while i < scratch.len() {
            let lit = scratch[i];
            if assignment.level(lit.var()).is_ground()
                && assignment.value(lit.var()) != Value::Unassigned
            {
                if assignment.value_of(lit) == Value::True {
                    subsumed = true;
                    break;
                }
                let _ = scratch.swap_remove(i);
                continue;
            }
            match assignment.value_of(lit) {
                Value::True => {
                    shrink_len = Some(i + 1);
                    break;
                }
                Value::False => {
                    let _ = scratch.swap_remove(i);
                    continue;
                }
                Value::Unassigned => {}
            }
            assignment.push_decision_level();
            let lvl = assignment.current_level();
            assignment.assign(!lit, Reason::decision(), lvl);
            let before = assignment.trail_len();
            let conflict = propagate(arena, assignment, long_watchers, bin_watchers);
            let after = assignment.trail_len();
            propagations = propagations.saturating_add((after - before) as u64);
            if conflict.is_some() {
                shrink_len = Some(i + 1);
                break;
            }
            i += 1;
        }
        backtrack_to(assignment, DecisionLevel::GROUND);

        if subsumed {
            arena.mark_deleted(cref);
            any_deleted = true;
            continue;
        }

        let original_len = arena.lits(cref).len();
        let new_len = shrink_len.unwrap_or(scratch.len());
        if new_len >= original_len {
            continue;
        }

        let new_lits = &scratch[..new_len];
        if new_len == 0 {
            return InprocessOutcome::Unsat;
        }

        arena.mark_deleted(cref);
        any_deleted = true;

        match new_len {
            1 => {
                let unit = new_lits[0];
                match assignment.value_of(unit) {
                    Value::True => {}
                    Value::False => return InprocessOutcome::Unsat,
                    Value::Unassigned => {
                        assignment.assign(unit, Reason::decision(), DecisionLevel::GROUND);
                    }
                }
            }
            2 => {
                attach_binary(bin_watchers, [new_lits[0], new_lits[1]]);
            }
            _ => {
                let new_lbd = compute_lbd(assignment, new_lits);
                if let Ok(new_id) = arena.push(new_lits, true, new_lbd) {
                    attach_long(long_watchers, arena, new_id);
                    learned_clauses.push(new_id);
                }
            }
        }
    }

    if any_deleted {
        for wl in long_watchers.iter_mut() {
            wl.retain(|w| !arena.is_deleted(w.clause));
        }
        learned_clauses.retain(|id| !arena.is_deleted(*id));
    }

    if propagate(arena, assignment, long_watchers, bin_watchers).is_some() {
        return InprocessOutcome::Unsat;
    }

    InprocessOutcome::Continue
}

/// Returns `true` when `cref` is currently the long-reason for some
/// literal assigned at the ground level. Mutating such a clause would
/// dangle the reason pointer.
fn is_locked_at_ground(arena: &ClauseArena, assignment: &Assignment, cref: ClauseId) -> bool {
    for &lit in arena.lits(cref) {
        if !assignment.level(lit.var()).is_ground() {
            continue;
        }
        if let Some(reason_id) = assignment.reason(lit.var()).as_long() {
            if reason_id == cref {
                return true;
            }
        }
    }
    false
}

/// Literal-block distance: number of distinct non-ground decision levels
/// appearing among the literals of `lits`. A ground-level literal does not
/// contribute to the LBD.
fn compute_lbd(assignment: &Assignment, lits: &[Lit]) -> u32 {
    let mut levels: Vec<u32> = Vec::with_capacity(lits.len());
    for lit in lits {
        let lvl = assignment.level(lit.var()).get();
        if lvl == 0 {
            continue;
        }
        if !levels.contains(&lvl) {
            levels.push(lvl);
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    let out = levels.len() as u32;
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::internal::watcher::{
        attach_long, ensure_binary_size, ensure_long_size, BinaryWatchers, LongWatchers,
    };
    use crate::types::Var;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    struct Harness {
        arena: ClauseArena,
        assignment: Assignment,
        lw: LongWatchers,
        bw: BinaryWatchers,
        learned: Vec<ClauseId>,
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
            }
        }

        fn add_learned_long(&mut self, lits: &[Lit], lbd: u32) -> ClauseId {
            let id = self.arena.push(lits, true, lbd).unwrap();
            attach_long(&mut self.lw, &self.arena, id);
            self.learned.push(id);
            id
        }

        fn run(&mut self) -> InprocessOutcome {
            vivify(
                &mut self.arena,
                &mut self.assignment,
                &mut self.lw,
                &mut self.bw,
                &mut self.learned,
                0,
            )
        }
    }

    #[test]
    fn no_learned_clauses_continues() {
        let mut h = Harness::new(2);
        assert_eq!(h.run(), InprocessOutcome::Continue);
    }

    #[test]
    fn high_lbd_clauses_ignored() {
        let mut h = Harness::new(4);
        // LBD above the cap is skipped; the pass leaves the clause alone.
        let id = h.add_learned_long(
            &[v(1).pos(), v(2).pos(), v(3).pos(), v(4).pos()],
            LBD_CAP + 1,
        );
        assert_eq!(h.run(), InprocessOutcome::Continue);
        assert!(!h.arena.is_deleted(id));
    }

    #[test]
    fn ground_level_true_literal_subsumes() {
        let mut h = Harness::new(3);
        h.assignment.assign(v(1).pos(), Reason::decision(), DecisionLevel::GROUND);
        let id = h.add_learned_long(&[v(1).pos(), v(2).pos(), v(3).pos()], 2);
        assert_eq!(h.run(), InprocessOutcome::Continue);
        assert!(h.arena.is_deleted(id));
    }
}
