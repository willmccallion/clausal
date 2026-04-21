//! Two-watched-literal boolean constraint propagation.
//!
//! Walks the trail from the propagation head forward, updating watches and
//! forcing unit assignments. Binary clauses are handled on a fast path that
//! never touches the arena. For long clauses we find a new non-false
//! literal to watch, or detect unit/conflict when none exists.

use core::mem;

use crate::internal::arena::ClauseArena;
use crate::internal::conflict::Conflict;
use crate::internal::reason::Reason;
use crate::internal::trail::Assignment;
use crate::internal::watcher::{BinaryWatchers, LongWatchers, Watcher};
use crate::types::Value;

/// Runs BCP until the assignment's propagation head catches up to the trail
/// tip, returning the first conflict encountered (if any).
///
/// The assignment's propagation head advances across every trail entry
/// consumed; on conflict return it points just past the literal whose
/// propagation produced the conflict.
pub(crate) fn propagate(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &BinaryWatchers,
) -> Option<Conflict> {
    while let Some(p) = assignment.take_next_to_propagate() {
        // Binary watches: no replacement is ever possible, so a simple
        // forward scan is enough. Index-based iteration avoids holding an
        // immutable borrow of the slot across `assign` calls below.
        let bin_len = bin_watchers[p.index()].len();
        let mut bi = 0usize;
        while bi < bin_len {
            let partner = bin_watchers[p.index()][bi].partner;
            bi += 1;
            match assignment.value_of(partner) {
                Value::True => {}
                Value::False => return Some(Conflict::Binary([!p, partner])),
                Value::Unassigned => {
                    let lvl = assignment.current_level();
                    assignment.assign(partner, Reason::binary(!p), lvl);
                }
            }
        }

        // Long watches: splice out clauses that find a replacement watched
        // literal. `write` trails `read`; dropped entries shorten the list.
        let mut list = mem::take(&mut long_watchers[p.index()]);
        let mut read = 0usize;
        let mut write = 0usize;
        let mut conflict: Option<Conflict> = None;
        let not_p = !p;

        'outer: while read < list.len() {
            let w = list[read];
            read += 1;

            // Blocker fast path: if a cached clause literal is already
            // true, the clause is satisfied and we keep the watch.
            if assignment.value_of(w.blocker) == Value::True {
                list[write] = w;
                write += 1;
                continue;
            }

            // Normalize the clause body so `lits[1] == not_p` (the newly
            // falsified watched literal) and `lits[0]` is the other.
            let lits = arena.lits_mut(w.clause);
            if lits[0] == not_p {
                lits.swap(0, 1);
            }
            let first = lits[0];

            // If the surviving watched literal is true the clause is
            // satisfied; keep the watch and refresh the blocker.
            if assignment.value_of(first) == Value::True {
                list[write] = Watcher { clause: w.clause, blocker: first };
                write += 1;
                continue;
            }

            // Search `lits[2..]` for a non-false replacement literal.
            let mut replacement: Option<usize> = None;
            let n = lits.len();
            let mut k = 2;
            while k < n {
                if assignment.value_of(lits[k]) != Value::False {
                    replacement = Some(k);
                    break;
                }
                k += 1;
            }

            if let Some(k) = replacement {
                lits.swap(1, k);
                let new_lit = lits[1];
                long_watchers[(!new_lit).index()].push(Watcher {
                    clause: w.clause,
                    blocker: first,
                });
                // Move on without copying w to the write slot.
                continue;
            }

            // No replacement: the clause is either unit or falsified.
            list[write] = Watcher { clause: w.clause, blocker: first };
            write += 1;

            match assignment.value_of(first) {
                Value::False => {
                    // Conflict. Preserve the rest of the watch list verbatim.
                    while read < list.len() {
                        list[write] = list[read];
                        read += 1;
                        write += 1;
                    }
                    conflict = Some(Conflict::LongClause(w.clause));
                    break 'outer;
                }
                Value::Unassigned => {
                    arena.set_used(w.clause);
                    let lvl = assignment.current_level();
                    assignment.assign(first, Reason::long(w.clause), lvl);
                }
                Value::True => {
                    // Impossible: ruled out by the earlier check.
                }
            }
        }

        list.truncate(write);
        long_watchers[p.index()] = list;

        if let Some(c) = conflict {
            return Some(c);
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use alloc::vec::Vec;

    use crate::internal::watcher::{
        attach_binary, attach_long, ensure_binary_size, ensure_long_size, BinaryWatchers,
        LongWatchers,
    };
    use crate::types::{DecisionLevel, Lit, Var};

    struct Harness {
        arena: ClauseArena,
        assignment: Assignment,
        lw: LongWatchers,
        bw: BinaryWatchers,
    }

    impl Harness {
        fn new(num_vars: u32) -> Self {
            let mut assignment = Assignment::new();
            for n in 1..=num_vars {
                assignment.ensure_var(Var::new(n).unwrap());
            }
            let mut lw: LongWatchers = Vec::new();
            let mut bw: BinaryWatchers = Vec::new();
            ensure_long_size(&mut lw, num_vars as usize);
            ensure_binary_size(&mut bw, num_vars as usize);
            Self { arena: ClauseArena::new(), assignment, lw, bw }
        }

        fn add_binary(&mut self, a: Lit, b: Lit) {
            attach_binary(&mut self.bw, [a, b]);
        }

        fn add_long(&mut self, lits: &[Lit]) {
            let id = self.arena.push(lits, false, 0).unwrap();
            attach_long(&mut self.lw, &self.arena, id);
        }

        fn decide(&mut self, lit: Lit) {
            self.assignment.push_decision_level();
            let lvl = self.assignment.current_level();
            self.assignment.assign(lit, Reason::decision(), lvl);
        }

        fn run(&mut self) -> Option<Conflict> {
            propagate(&mut self.arena, &mut self.assignment, &mut self.lw, &self.bw)
        }
    }

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    #[test]
    fn empty_trail_no_work() {
        let mut h = Harness::new(3);
        assert!(h.run().is_none());
    }

    #[test]
    fn binary_unit_propagation() {
        // Clause: (!x1 v x2). Decide x1=true. x2 must become true.
        let mut h = Harness::new(2);
        h.add_binary(v(1).neg(), v(2).pos());
        h.decide(v(1).pos());
        assert!(h.run().is_none());
        assert_eq!(h.assignment.value(v(2)), Value::True);
        assert!(h.assignment.reason(v(2)).is_binary());
    }

    #[test]
    fn binary_conflict() {
        // Clauses: (!x1 v x2), (!x1 v !x2). Deciding x1=true forces both
        // polarities of x2, which should conflict on the second binary.
        let mut h = Harness::new(2);
        h.add_binary(v(1).neg(), v(2).pos());
        h.add_binary(v(1).neg(), v(2).neg());
        h.decide(v(1).pos());
        match h.run() {
            Some(Conflict::Binary(_)) => {}
            other => panic!("expected binary conflict, got {other:?}"),
        }
    }

    #[test]
    fn long_blocker_satisfies() {
        // Clause: (x1 v x2 v x3). Decide x1=true first (blocker hit path
        // won't fire until !x1 is propagated; here we just make sure that
        // decisions don't trip the watchers incorrectly).
        let mut h = Harness::new(3);
        h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        h.decide(v(1).pos());
        assert!(h.run().is_none());
        assert_eq!(h.assignment.value(v(2)), Value::Unassigned);
        assert_eq!(h.assignment.value(v(3)), Value::Unassigned);
    }

    #[test]
    fn long_unit_propagation() {
        // Clause: (x1 v x2 v x3). Assign x1=false, x2=false. x3 must be true.
        let mut h = Harness::new(3);
        h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        h.decide(v(1).neg());
        assert!(h.run().is_none());
        h.decide(v(2).neg());
        assert!(h.run().is_none());
        assert_eq!(h.assignment.value(v(3)), Value::True);
        assert!(h.assignment.reason(v(3)).as_long().is_some());
    }

    #[test]
    fn long_replacement_avoids_unit() {
        // Clause: (x1 v x2 v x3 v x4). Falsify x1, then x2. The watcher
        // should slide to x3 or x4 instead of producing a unit.
        let mut h = Harness::new(4);
        h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos(), v(4).pos()]);
        h.decide(v(1).neg());
        assert!(h.run().is_none());
        h.decide(v(2).neg());
        assert!(h.run().is_none());
        assert_eq!(h.assignment.value(v(3)), Value::Unassigned);
        assert_eq!(h.assignment.value(v(4)), Value::Unassigned);
    }

    #[test]
    fn long_conflict() {
        // (x1 v x2 v x3) and (x1 v x2 v !x3). After falsifying x1 and x2
        // the first propagates x3=true; visiting x3=true's watchers finds
        // the second clause with every literal false and reports conflict.
        let mut h = Harness::new(3);
        h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        h.add_long(&[v(1).pos(), v(2).pos(), v(3).neg()]);
        h.decide(v(1).neg());
        assert!(h.run().is_none());
        h.decide(v(2).neg());
        match h.run() {
            Some(Conflict::LongClause(_)) => {}
            other => panic!("expected long conflict, got {other:?}"),
        }
    }

    #[test]
    fn cascading_propagation() {
        // (!x1 v x2) and (!x2 v x3). Deciding x1=true should force x2 then x3.
        let mut h = Harness::new(3);
        h.add_binary(v(1).neg(), v(2).pos());
        h.add_binary(v(2).neg(), v(3).pos());
        h.decide(v(1).pos());
        assert!(h.run().is_none());
        assert_eq!(h.assignment.value(v(2)), Value::True);
        assert_eq!(h.assignment.value(v(3)), Value::True);
    }

    #[test]
    fn unit_at_ground_level() {
        // (x1). Assign directly at ground, no watchers needed for units.
        let mut h = Harness::new(1);
        h.assignment.assign(v(1).pos(), Reason::decision(), DecisionLevel::GROUND);
        assert!(h.run().is_none());
        assert_eq!(h.assignment.value(v(1)), Value::True);
    }

    #[test]
    fn blocker_short_circuit() {
        // Clause: (x1 v x2 v x3). First falsify x3 (triggers the
        // watcher-of-x3 scan but lits[0], lits[1] are both unassigned, so
        // no unit). Then assign x2 true. Then falsify x1: blocker is
        // cached from when we last visited the clause; x2=true should
        // short-circuit the scan via the blocker.
        let mut h = Harness::new(3);
        h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        h.decide(v(3).neg());
        assert!(h.run().is_none());
        h.decide(v(2).pos());
        assert!(h.run().is_none());
        h.decide(v(1).neg());
        assert!(h.run().is_none(), "blocker-satisfied clause must not fire");
    }
}
