//! Learned-clause database reduction and arena compaction.
//!
//! As the solver runs, learned clauses accumulate in the arena and on the
//! watch lists. A budget-driven reduction pass periodically deletes the
//! half of the learned clauses with the worst (highest) LBDs, skipping
//! clauses that are currently the reason for a trail assignment (locked)
//! and clauses that are rated "glue" (LBD at or below a small ceiling).
//! Every few reductions an arena compaction pass walks the surviving
//! clauses into a fresh back buffer, rewriting watchers, learned-clause
//! references, and long-clause reasons through a remap table.
//!
//! The engine-visible API is [`ReduceState`], [`reduce_learned`], and
//! [`compact`]; the search loop consults the first to decide when to call
//! the other two.

use alloc::vec::Vec;

use crate::error::Result;
use crate::internal::arena::ClauseArena;
use crate::internal::trail::Assignment;
use crate::internal::watcher::LongWatchers;
use crate::types::ClauseId;

/// Conflicts before the first reduction fires.
const REDUCE_INITIAL: u64 = 2_000;
/// Amount the inter-reduction interval grows after each reduction.
const REDUCE_GROW: u64 = 300;
/// Number of reductions that must run before an arena compaction is scheduled.
const COMPACT_EVERY: u64 = 4;
/// LBD at or below this ceiling keeps a learned clause safe from reduction.
const GLUE_CEILING: u32 = 2;

/// Rolling state driving the reduction schedule.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReduceState {
    /// The next conflict count at which a reduction should fire.
    pub(crate) next_reduce: u64,
    /// Current interval between reductions; grows by `REDUCE_GROW` each pass.
    pub(crate) interval: u64,
    /// Number of reductions run so far (used to schedule compaction).
    pub(crate) reductions: u64,
}

impl ReduceState {
    /// Creates a fresh reduce state with the initial budget.
    pub(crate) const fn new() -> Self {
        Self { next_reduce: REDUCE_INITIAL, interval: REDUCE_INITIAL, reductions: 0 }
    }

    /// Returns `true` if enough conflicts have elapsed to trigger a reduction.
    pub(crate) const fn should_reduce(&self, conflicts: u64) -> bool {
        conflicts >= self.next_reduce
    }

    /// Advances the schedule after a reduction has fired.
    pub(crate) const fn on_reduced(&mut self) {
        self.reductions = self.reductions.saturating_add(1);
        self.interval = self.interval.saturating_add(REDUCE_GROW);
        self.next_reduce = self.next_reduce.saturating_add(self.interval);
    }

    /// Returns `true` if, after the most recent reduction, an arena
    /// compaction pass should also run.
    pub(crate) const fn should_compact(&self) -> bool {
        self.reductions > 0 && self.reductions % COMPACT_EVERY == 0
    }
}

impl Default for ReduceState {
    fn default() -> Self {
        Self::new()
    }
}

/// Marks the worst half of learned clauses deleted, then sweeps watchers
/// and the learned-clause list to drop dangling references.
///
/// Clauses currently serving as an assignment's reason are skipped
/// (locked), as are glue clauses with LBD at or below [`GLUE_CEILING`].
pub(crate) fn reduce_learned(
    arena: &mut ClauseArena,
    long_watchers: &mut LongWatchers,
    learned_clauses: &mut Vec<ClauseId>,
    assignment: &Assignment,
) {
    let mut locked_slots: Vec<usize> = Vec::new();
    for lit in assignment.trail() {
        if let Some(id) = assignment.reason(lit.var()).as_long() {
            locked_slots.push(ClauseArena::slot_of(id));
        }
    }
    locked_slots.sort_unstable();
    locked_slots.dedup();
    let is_locked = |id: ClauseId| -> bool {
        locked_slots.binary_search(&ClauseArena::slot_of(id)).is_ok()
    };

    let mut candidates: Vec<ClauseId> = learned_clauses
        .iter()
        .copied()
        .filter(|&id| {
            !arena.is_deleted(id) && arena.lbd(id) > GLUE_CEILING && !is_locked(id)
        })
        .collect();

    candidates.sort_unstable_by_key(|&id| core::cmp::Reverse(arena.lbd(id)));

    let half = candidates.len() / 2;
    for &id in &candidates[..half] {
        arena.mark_deleted(id);
    }

    for watch_list in long_watchers.iter_mut() {
        watch_list.retain(|w| !arena.is_deleted(w.clause));
    }
    learned_clauses.retain(|id| !arena.is_deleted(*id));
}

/// Runs an arena compaction pass: rewrites the arena in place, dropping
/// deleted clauses, and migrates every outward reference (watchers,
/// learned-clause list, long-clause reasons) through the remap table.
pub(crate) fn compact(
    arena: &mut ClauseArena,
    long_watchers: &mut LongWatchers,
    learned_clauses: &mut Vec<ClauseId>,
    assignment: &mut Assignment,
) -> Result<()> {
    let remap = arena.compact()?;

    for watch_list in &mut *long_watchers {
        watch_list.retain_mut(|w| {
            let slot = ClauseArena::slot_of(w.clause);
            if let Some(new_id) = remap.get(slot).copied().flatten() {
                w.clause = new_id;
                true
            } else {
                false
            }
        });
    }

    learned_clauses.retain_mut(|id| {
        let slot = ClauseArena::slot_of(*id);
        remap.get(slot).copied().flatten().is_some_and(|new_id| {
            *id = new_id;
            true
        })
    });

    assignment.remap_long_reasons(|id| {
        let slot = ClauseArena::slot_of(id);
        // Reason clauses are locked, so they always survive compaction.
        // Fall back to the original id if the remap entry is absent; the
        // reason will simply be overwritten next time the literal reassigns.
        remap.get(slot).copied().flatten().unwrap_or(id)
    });

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::internal::reason::Reason;
    use crate::internal::watcher::{attach_long, ensure_long_size};
    use crate::types::{DecisionLevel, Lit, Var};

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    fn setup(num_vars: u32) -> (ClauseArena, Assignment, LongWatchers, Vec<ClauseId>) {
        let mut assignment = Assignment::new();
        for n in 1..=num_vars {
            assignment.ensure_var(v(n));
        }
        let mut lw: LongWatchers = Vec::new();
        ensure_long_size(&mut lw, num_vars as usize);
        (ClauseArena::new(), assignment, lw, Vec::new())
    }

    fn push_learned(
        arena: &mut ClauseArena,
        lw: &mut LongWatchers,
        learned: &mut Vec<ClauseId>,
        lits: &[Lit],
        lbd: u32,
    ) -> ClauseId {
        let id = arena.push(lits, true, lbd).unwrap();
        attach_long(lw, arena, id);
        learned.push(id);
        id
    }

    #[test]
    fn schedule_fires_at_budget() {
        let s = ReduceState::new();
        assert!(!s.should_reduce(REDUCE_INITIAL - 1));
        assert!(s.should_reduce(REDUCE_INITIAL));
    }

    #[test]
    fn schedule_grows_interval() {
        let mut s = ReduceState::new();
        s.on_reduced();
        assert_eq!(s.interval, REDUCE_INITIAL + REDUCE_GROW);
        assert_eq!(s.next_reduce, REDUCE_INITIAL + REDUCE_INITIAL + REDUCE_GROW);
    }

    #[test]
    fn compaction_schedule_hits_every_four() {
        let mut s = ReduceState::new();
        for _ in 0..3 {
            s.on_reduced();
            assert!(!s.should_compact());
        }
        s.on_reduced();
        assert!(s.should_compact());
    }

    #[test]
    fn reduce_deletes_worst_half_of_non_glue() {
        let (mut arena, assignment, mut lw, mut learned) = setup(18);
        // Six learned long clauses with LBDs 3..=8. None are glue, none locked.
        for (i, lbd) in (3u32..=8).enumerate() {
            let offset = u32::try_from(i).unwrap() * 3;
            let a = Var::new(offset + 1).unwrap().pos();
            let b = Var::new(offset + 2).unwrap().pos();
            let c = Var::new(offset + 3).unwrap().neg();
            let _ = push_learned(&mut arena, &mut lw, &mut learned, &[a, b, c], lbd);
        }
        let before = learned.len();
        reduce_learned(&mut arena, &mut lw, &mut learned, &assignment);
        assert!(learned.len() < before, "some learned clauses must be dropped");
        // The worst half (LBDs 6, 7, 8) should be gone; LBDs 3, 4, 5 remain.
        for id in &learned {
            assert!(arena.lbd(*id) <= 5);
        }
    }

    #[test]
    fn reduce_skips_glue_clauses() {
        let (mut arena, assignment, mut lw, mut learned) = setup(9);
        // Three glue (LBD=2) and three non-glue (LBD=5).
        for i in 0..3u32 {
            let offset = i * 3;
            let a = Var::new(offset + 1).unwrap().pos();
            let b = Var::new(offset + 2).unwrap().pos();
            let c = Var::new(offset + 3).unwrap().neg();
            let _ = push_learned(&mut arena, &mut lw, &mut learned, &[a, b, c], 2);
        }
        let glue_count = learned.len();
        reduce_learned(&mut arena, &mut lw, &mut learned, &assignment);
        // No non-glue candidates existed, so nothing is deleted.
        assert_eq!(learned.len(), glue_count);
    }

    #[test]
    fn reduce_preserves_locked_clauses() {
        let (mut arena, mut assignment, mut lw, mut learned) = setup(6);
        // First clause: LBD 10, reason for an assignment (locked).
        let a = v(1).pos();
        let locked_id = push_learned(
            &mut arena,
            &mut lw,
            &mut learned,
            &[a, v(2).pos(), v(3).neg()],
            10,
        );
        assignment.assign(a, Reason::long(locked_id), DecisionLevel::GROUND);
        // Three decoys with better (lower) LBDs. Without the locked guard
        // the worst half would drop the LBD-10 clause.
        let _ = push_learned(&mut arena, &mut lw, &mut learned, &[v(4).pos(), v(5).pos(), v(6).neg()], 4);
        let _ = push_learned(&mut arena, &mut lw, &mut learned, &[v(4).neg(), v(5).pos(), v(6).pos()], 5);
        let _ = push_learned(&mut arena, &mut lw, &mut learned, &[v(4).neg(), v(5).neg(), v(6).pos()], 6);
        reduce_learned(&mut arena, &mut lw, &mut learned, &assignment);
        assert!(
            learned.contains(&locked_id),
            "locked clause must survive reduction even though its LBD is the worst",
        );
    }

    #[test]
    #[allow(clippy::many_single_char_names, reason = "literals named after their variable indices")]
    fn compact_removes_deleted_and_rewrites_watchers() {
        let (mut arena, mut assignment, mut lw, mut learned) = setup(6);
        // Two learned clauses; mark the first deleted, then compact.
        let a = v(1).pos();
        let b = v(2).pos();
        let c = v(3).neg();
        let d = v(4).pos();
        let e = v(5).pos();
        let f = v(6).neg();
        let id_a = push_learned(&mut arena, &mut lw, &mut learned, &[a, b, c], 3);
        let _id_b = push_learned(&mut arena, &mut lw, &mut learned, &[d, e, f], 4);
        arena.mark_deleted(id_a);
        learned.retain(|x| *x != id_a);
        // Pre-compaction: both clauses still in arena, watcher lists have
        // references to both (but the deleted one should be swept).
        for wl in &mut lw {
            wl.retain(|w| !arena.is_deleted(w.clause));
        }
        compact(&mut arena, &mut lw, &mut learned, &mut assignment).unwrap();
        assert_eq!(arena.num_clauses(), 1);
        // The surviving id is now the first slot, not id_b's original slot.
        let surviving = learned[0];
        assert_eq!(ClauseArena::slot_of(surviving), 0);
        // Watchers for vars 4, 5 now reference the surviving id.
        let wl_na = &lw[(!d).index()];
        assert!(wl_na.iter().any(|w| w.clause == surviving));
    }
}
