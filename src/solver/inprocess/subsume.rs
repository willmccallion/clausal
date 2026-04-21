//! Clause subsumption and self-subsuming resolution (SSR).
//!
//! This pass keeps a conservative, occurrence-list driven flavour:
//! clauses are fingerprinted with a 64-bit signature, and the pass walks
//! every clause pair that shares at least one literal to check whether
//! one subsumes the other (or strengthens via SSR).
//!
//! - **Subsumption:** clause `C` subsumes `D` if every literal of `C`
//!   also appears in `D`. `D` is then redundant and marked deleted.
//! - **Self-subsuming resolution:** `C` strengthens `D` if every literal
//!   of `C` except one, say `x`, appears in `D`, and the negation of
//!   `x` appears in `D`. Then `D` can drop the `¬x` literal.
//!
//! Because SSR can reduce a long clause to a binary or a unit, the pass
//! reinstalls the shortened clause and propagates afterwards. This
//! implementation only rewrites long clauses; binary subsumption is
//! handled by `BinaryWatchers` updates.

use alloc::vec::Vec;

use crate::internal::arena::ClauseArena;
use crate::internal::reason::Reason;
use crate::internal::trail::Assignment;
use crate::internal::watcher::{
    attach_binary, attach_long, BinaryWatchers, LongWatchers,
};
use crate::solver::inprocess::InprocessOutcome;
use crate::solver::search::propagate::propagate;
use crate::types::{ClauseId, DecisionLevel, Lit, Value};

/// Maximum clause length subsumption will consider as a potential
/// subsumer. Longer clauses are skipped to keep the pass near-linear.
const SUBSUMER_LEN_CAP: u32 = 32;

/// Runs a subsumption/SSR pass at the ground decision level.
///
/// Returns [`InprocessOutcome::Unsat`] if SSR strengthens any clause to
/// the empty clause or to a conflicting unit.
pub(crate) fn subsume(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &mut BinaryWatchers,
    learned_clauses: &mut Vec<ClauseId>,
    num_vars: u32,
) -> InprocessOutcome {
    debug_assert!(assignment.current_level().is_ground());
    if arena.num_clauses() == 0 || num_vars == 0 {
        return InprocessOutcome::Continue;
    }

    // Build an occurrence list indexed by literal.
    let mut occurs: Vec<Vec<ClauseId>> = (0..2 * num_vars as usize).map(|_| Vec::new()).collect();
    let mut signatures: Vec<u64> = Vec::with_capacity(arena.num_clauses());
    let mut ids: Vec<ClauseId> = Vec::with_capacity(arena.num_clauses());
    for slot in 0..arena.num_clauses() {
        let id = match core::num::NonZeroU32::new(
            u32::try_from(slot + 1).unwrap_or(u32::MAX),
        ) {
            Some(nz) => ClauseId::from_raw(nz),
            None => continue,
        };
        if arena.is_deleted(id) {
            signatures.push(0);
            ids.push(id);
            continue;
        }
        let lits = arena.lits(id);
        if lits.len() < 2 {
            signatures.push(0);
            ids.push(id);
            continue;
        }
        let mut sig: u64 = 0;
        for &lit in lits {
            // Variable-indexed bloom: a flipped literal still matches, which
            // is what self-subsuming resolution needs.
            sig |= 1u64 << (u64::from(lit.var().index() as u32) & 63);
            if let Some(bucket) = occurs.get_mut(lit.index()) {
                bucket.push(id);
            }
        }
        signatures.push(sig);
        ids.push(id);
    }

    let mut any_deleted = false;
    let mut derived_unit = false;
    let mut derived_conflict = false;

    let mut work_adds: Vec<Vec<Lit>> = Vec::new();

    let num_clauses = arena.num_clauses();
    for slot_a in 0..num_clauses {
        if derived_conflict {
            break;
        }
        let id_a = ids[slot_a];
        if arena.is_deleted(id_a) {
            continue;
        }
        let lits_a: Vec<Lit> = arena.lits(id_a).to_vec();
        if lits_a.len() < 2 || (lits_a.len() as u32) > SUBSUMER_LEN_CAP {
            continue;
        }
        let sig_a = signatures[slot_a];

        // Pick the rarest literal to iterate over its occurrence list.
        let mut best_lit = lits_a[0];
        let mut best_count = occurs.get(best_lit.index()).map_or(usize::MAX, Vec::len);
        for &lit in &lits_a[1..] {
            let c = occurs.get(lit.index()).map_or(usize::MAX, Vec::len);
            if c < best_count {
                best_count = c;
                best_lit = lit;
            }
        }

        // Walk `best_lit` and `!best_lit`: the first picks up subsumption
        // candidates (clauses containing the same literal) and the second
        // picks up SSR candidates where `best_lit` is the flipped literal.
        // Any other flip in `a` still leaves `best_lit` in the victim, so
        // the positive list covers those too.
        let mut occ_snapshot: Vec<ClauseId> = occurs
            .get(best_lit.index())
            .cloned()
            .unwrap_or_default();
        if let Some(neg_bucket) = occurs.get((!best_lit).index()) {
            occ_snapshot.extend_from_slice(neg_bucket);
        }
        for id_b in occ_snapshot {
            if id_b == id_a {
                continue;
            }
            if arena.is_deleted(id_b) {
                continue;
            }
            let slot_b = ClauseArena::slot_of(id_b);
            let sig_b = signatures[slot_b];
            if (sig_a & !sig_b) != 0 {
                continue;
            }
            let lits_b: Vec<Lit> = arena.lits(id_b).to_vec();
            if lits_b.len() < lits_a.len() {
                continue;
            }
            match classify(&lits_a, &lits_b) {
                Match::Subsumes => {
                    arena.mark_deleted(id_b);
                    any_deleted = true;
                }
                Match::Strengthen(removed) => {
                    // Strengthen: new clause = lits_b without `removed`.
                    let new_lits: Vec<Lit> =
                        lits_b.iter().copied().filter(|&l| l != removed).collect();
                    arena.mark_deleted(id_b);
                    any_deleted = true;
                    if new_lits.is_empty() {
                        derived_conflict = true;
                        break;
                    }
                    if new_lits.len() == 1 {
                        let unit = new_lits[0];
                        match assignment.value_of(unit) {
                            Value::True => {}
                            Value::False => {
                                derived_conflict = true;
                                break;
                            }
                            Value::Unassigned => {
                                assignment.assign(
                                    unit,
                                    Reason::decision(),
                                    DecisionLevel::GROUND,
                                );
                                derived_unit = true;
                            }
                        }
                        continue;
                    }
                    work_adds.push(new_lits);
                }
                Match::None => {}
            }
        }
    }

    if any_deleted {
        for wl in long_watchers.iter_mut() {
            wl.retain(|w| !arena.is_deleted(w.clause));
        }
        learned_clauses.retain(|id| !arena.is_deleted(*id));
    }

    if derived_conflict {
        return InprocessOutcome::Unsat;
    }

    for new_lits in work_adds {
        if new_lits.len() == 2 {
            attach_binary(bin_watchers, [new_lits[0], new_lits[1]]);
        } else if let Ok(new_id) = arena.push(&new_lits, false, 0) {
            attach_long(long_watchers, arena, new_id);
        }
    }

    if derived_unit
        && propagate(arena, assignment, long_watchers, bin_watchers).is_some()
    {
        return InprocessOutcome::Unsat;
    }

    InprocessOutcome::Continue
}

/// Relationship between two clauses used by subsume.
enum Match {
    /// `a` is a subset of `b`.
    Subsumes,
    /// `a` is a subset of `b` after flipping exactly one literal's polarity;
    /// the flipped literal (in its `b`-side polarity) is returned.
    Strengthen(Lit),
    /// `a` is not a subset and not a near-subset of `b`.
    None,
}

/// Classifies `a` against `b`.
///
/// Implementation runs in `O(|a| * |b|)` which is fine for the
/// length-capped setting of [`SUBSUMER_LEN_CAP`].
fn classify(a: &[Lit], b: &[Lit]) -> Match {
    let mut mismatch: Option<Lit> = None;
    for &la in a {
        if b.contains(&la) {
            continue;
        }
        if b.contains(&!la) {
            if mismatch.is_some() {
                return Match::None;
            }
            mismatch = Some(!la);
            continue;
        }
        return Match::None;
    }
    match mismatch {
        None => Match::Subsumes,
        Some(l) => Match::Strengthen(l),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::internal::watcher::{
        attach_long, ensure_binary_size, ensure_long_size,
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
                learned: Vec::new(),
                num_vars,
            }
        }

        fn add_long(&mut self, lits: &[Lit]) -> ClauseId {
            let id = self.arena.push(lits, false, 0).unwrap();
            attach_long(&mut self.lw, &self.arena, id);
            id
        }

        fn run(&mut self) -> InprocessOutcome {
            subsume(
                &mut self.arena,
                &mut self.assignment,
                &mut self.lw,
                &mut self.bw,
                &mut self.learned,
                self.num_vars,
            )
        }
    }

    #[test]
    fn empty_is_continue() {
        let mut h = Harness::new(0);
        assert_eq!(h.run(), InprocessOutcome::Continue);
    }

    #[test]
    fn subsumed_clause_is_deleted() {
        let mut h = Harness::new(3);
        let short = h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        // A longer clause that contains every literal of `short` is subsumed.
        let long_id = h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos(), v(3).pos()]);
        // An unrelated clause that shares no subsume/SSR shape with `short`:
        // two mismatched literals rules out both subsumption and SSR.
        let other = h.add_long(&[v(1).neg(), v(2).neg(), v(3).pos()]);
        assert_eq!(h.run(), InprocessOutcome::Continue);
        assert!(h.arena.is_deleted(long_id));
        assert!(!h.arena.is_deleted(short));
        assert!(!h.arena.is_deleted(other));
    }

    #[test]
    fn ssr_strengthens_long_clause() {
        // (x1 v x2 v x3) and (!x1 v x2 v x3 v x4). The second can drop !x1,
        // producing (x2 v x3 v x4), which is shorter.
        let mut h = Harness::new(4);
        let _short = h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        let victim = h.add_long(&[v(1).neg(), v(2).pos(), v(3).pos(), v(4).pos()]);
        assert_eq!(h.run(), InprocessOutcome::Continue);
        assert!(h.arena.is_deleted(victim));
        // A new clause should have been installed.
        assert_eq!(h.arena.num_clauses(), 3);
    }
}
