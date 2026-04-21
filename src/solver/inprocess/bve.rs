//! Bounded variable elimination.
//!
//! This pass handles the easiest-to-commit slice of BVE: pure-literal
//! elimination, where a variable appears only in one polarity across
//! every live clause. The variable is then asserted in that polarity as
//! a ground-level unit, every occurrence is trivially satisfied, and
//! model reconstruction is free (the ground-level assignment is the
//! answer).
//!
//! A full resolution-based BVE with growth gating, witness frames, and
//! per-pass budget accounting is tracked separately; keeping this pass
//! conservative preserves soundness without risking clause blow-up.

use alloc::vec::Vec;

use crate::internal::arena::ClauseArena;
use crate::internal::reason::Reason;
use crate::internal::trail::Assignment;
use crate::internal::watcher::{BinaryWatchers, LongWatchers};
use crate::solver::inprocess::InprocessOutcome;
use crate::solver::search::propagate::propagate;
use crate::types::{ClauseId, DecisionLevel, Value, Var};

/// Runs a pure-literal elimination pass at the ground decision level.
pub(crate) fn bve(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &mut BinaryWatchers,
    num_vars: u32,
) -> InprocessOutcome {
    debug_assert!(assignment.current_level().is_ground());
    if num_vars == 0 {
        return InprocessOutcome::Continue;
    }

    let num_lits = num_vars as usize * 2;
    let mut pos_count: Vec<u32> = alloc::vec![0; num_vars as usize];
    let mut neg_count: Vec<u32> = alloc::vec![0; num_vars as usize];

    for slot in 0..arena.num_clauses() {
        let Some(nz) = core::num::NonZeroU32::new(u32::try_from(slot + 1).unwrap_or(u32::MAX))
        else {
            continue;
        };
        let id = ClauseId::from_raw(nz);
        if arena.is_deleted(id) {
            continue;
        }
        let lits = arena.lits(id);
        // Skip clauses already satisfied at the ground level.
        if lits.iter().any(|&l| assignment.value_of(l) == Value::True) {
            continue;
        }
        for &lit in lits {
            if assignment.value_of(lit) == Value::False {
                continue;
            }
            let vi = lit.var().index();
            if lit.is_positive() {
                pos_count[vi] = pos_count[vi].saturating_add(1);
            } else {
                neg_count[vi] = neg_count[vi].saturating_add(1);
            }
        }
    }

    // Walk binary watchers. Each binary (a, b) lives at bin_watchers[!a]
    // and bin_watchers[!b]; iterate once per pair via raw ordering.
    for raw in 0..num_lits as u32 {
        let Some(a_neg) = lit_from_index(raw) else {
            continue;
        };
        let a = !a_neg;
        if let Some(list) = bin_watchers.get(raw as usize) {
            for entry in list {
                let b = entry.partner;
                if a.to_raw() >= b.to_raw() {
                    continue;
                }
                // Drop if either literal is already true at ground.
                if assignment.value_of(a) == Value::True
                    || assignment.value_of(b) == Value::True
                {
                    continue;
                }
                for lit in [a, b] {
                    if assignment.value_of(lit) == Value::False {
                        continue;
                    }
                    let vi = lit.var().index();
                    if lit.is_positive() {
                        pos_count[vi] = pos_count[vi].saturating_add(1);
                    } else {
                        neg_count[vi] = neg_count[vi].saturating_add(1);
                    }
                }
            }
        }
    }

    let mut any_assigned = false;
    for n in 1..=num_vars {
        let Some(var) = Var::new(n) else { continue };
        if assignment.value(var) != Value::Unassigned {
            continue;
        }
        let vi = var.index();
        let p = pos_count[vi];
        let neg = neg_count[vi];
        if p > 0 && neg == 0 {
            assignment.assign(var.pos(), Reason::decision(), DecisionLevel::GROUND);
            any_assigned = true;
        } else if neg > 0 && p == 0 {
            assignment.assign(var.neg(), Reason::decision(), DecisionLevel::GROUND);
            any_assigned = true;
        }
    }

    if any_assigned
        && propagate(arena, assignment, long_watchers, bin_watchers).is_some()
    {
        return InprocessOutcome::Unsat;
    }

    InprocessOutcome::Continue
}

fn lit_from_index(raw: u32) -> Option<crate::types::Lit> {
    let plus2 = raw.checked_add(2)?;
    crate::types::Lit::from_raw(plus2)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::internal::watcher::{
        attach_binary, attach_long, ensure_binary_size, ensure_long_size,
    };
    use crate::types::Lit;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    struct Harness {
        arena: ClauseArena,
        assignment: Assignment,
        lw: LongWatchers,
        bw: BinaryWatchers,
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
            Self {
                arena: ClauseArena::new(),
                assignment,
                lw,
                bw,
                num_vars,
            }
        }

        fn add_long(&mut self, lits: &[Lit]) -> ClauseId {
            let id = self.arena.push(lits, false, 0).unwrap();
            attach_long(&mut self.lw, &self.arena, id);
            id
        }

        fn add_binary(&mut self, a: Lit, b: Lit) {
            attach_binary(&mut self.bw, [a, b]);
        }

        fn run(&mut self) -> InprocessOutcome {
            bve(
                &mut self.arena,
                &mut self.assignment,
                &mut self.lw,
                &mut self.bw,
                self.num_vars,
            )
        }
    }

    #[test]
    fn pure_positive_is_assigned_true() {
        let mut h = Harness::new(3);
        let _ = h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        h.add_binary(v(1).pos(), v(2).neg());
        assert_eq!(h.run(), InprocessOutcome::Continue);
        assert_eq!(h.assignment.value(v(1)), Value::True);
        assert_eq!(h.assignment.value(v(3)), Value::True);
    }

    #[test]
    fn mixed_polarity_is_left_alone() {
        let mut h = Harness::new(2);
        h.add_binary(v(1).pos(), v(2).pos());
        h.add_binary(v(1).neg(), v(2).neg());
        assert_eq!(h.run(), InprocessOutcome::Continue);
        assert_eq!(h.assignment.value(v(1)), Value::Unassigned);
        assert_eq!(h.assignment.value(v(2)), Value::Unassigned);
    }
}
