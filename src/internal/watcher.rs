//! Two-watched-literal data structures.
//!
//! Watches are indexed by literal: `watchers[p.index()]` holds the
//! watch-list visited when the literal `p` becomes false, i.e. the clauses
//! that watch `!p` as one of their two watched literals. Binary watches are
//! kept in a parallel table so the BCP hot loop can fast-path them before
//! touching the long-clause arena.

use alloc::vec::Vec;

use crate::internal::arena::ClauseArena;
use crate::types::{ClauseId, Lit};

/// A watcher on a long (three-or-more literal) clause.
///
/// `blocker` is a cached literal from the clause used to skip the full
/// clause lookup when the blocker is already satisfied.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Watcher {
    pub(crate) clause: ClauseId,
    pub(crate) blocker: Lit,
}

/// A watcher for a binary clause. The partner literal is stored inline so
/// propagation can resolve the pair without visiting the arena.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BinaryWatch {
    pub(crate) partner: Lit,
}

/// Per-literal watch lists for long clauses.
pub(crate) type LongWatchers = Vec<Vec<Watcher>>;

/// Per-literal watch lists for binary clauses.
pub(crate) type BinaryWatchers = Vec<Vec<BinaryWatch>>;

/// Ensures `watchers` has one watch list per literal index for the given
/// variable count. Existing lists are preserved.
pub(crate) fn ensure_long_size(watchers: &mut LongWatchers, num_vars: usize) {
    let needed = num_vars * 2;
    if watchers.len() < needed {
        watchers.resize_with(needed, Vec::new);
    }
}

/// Ensures `watchers` has one watch list per literal index for the given
/// variable count. Existing lists are preserved.
pub(crate) fn ensure_binary_size(watchers: &mut BinaryWatchers, num_vars: usize) {
    let needed = num_vars * 2;
    if watchers.len() < needed {
        watchers.resize_with(needed, Vec::new);
    }
}

/// Installs watchers for a freshly-added long clause.
///
/// The first two literals in the clause body become the watched literals;
/// the caller must have arranged for them to be unassigned or true. The
/// clause is registered in the watch-lists indexed by the negation of each
/// watched literal, matching the convention that `long_watchers[p]` holds
/// clauses revisited when `p` is assigned true (i.e. the clauses whose
/// watched literal `!p` just became false).
pub(crate) fn attach_long(long_watchers: &mut LongWatchers, arena: &ClauseArena, id: ClauseId) {
    let lits = arena.lits(id);
    debug_assert!(lits.len() >= 3, "attach_long requires len >= 3");
    let a = lits[0];
    let b = lits[1];
    long_watchers[(!a).index()].push(Watcher { clause: id, blocker: b });
    long_watchers[(!b).index()].push(Watcher { clause: id, blocker: a });
}

/// Installs watchers for a freshly-added binary clause.
pub(crate) fn attach_binary(bin_watchers: &mut BinaryWatchers, lits: [Lit; 2]) {
    let [a, b] = lits;
    bin_watchers[(!a).index()].push(BinaryWatch { partner: b });
    bin_watchers[(!b).index()].push(BinaryWatch { partner: a });
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::types::Var;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    #[test]
    fn ensure_long_grows() {
        let mut lw: LongWatchers = Vec::new();
        ensure_long_size(&mut lw, 3);
        assert_eq!(lw.len(), 6);
        assert!(lw.iter().all(Vec::is_empty));
    }

    #[test]
    fn ensure_long_preserves_existing() {
        let mut lw: LongWatchers = Vec::new();
        ensure_long_size(&mut lw, 1);
        lw[v(1).pos().index()].push(Watcher {
            clause: crate::types::ClauseId::from_raw(core::num::NonZeroU32::new(1).unwrap()),
            blocker: v(1).pos(),
        });
        ensure_long_size(&mut lw, 3);
        assert_eq!(lw.len(), 6);
        assert_eq!(lw[v(1).pos().index()].len(), 1);
    }

    #[test]
    fn ensure_binary_grows() {
        let mut bw: BinaryWatchers = Vec::new();
        ensure_binary_size(&mut bw, 4);
        assert_eq!(bw.len(), 8);
    }
}
