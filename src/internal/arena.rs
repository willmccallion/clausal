//! Flat clause arena backed by struct-of-arrays storage.
//!
//! Per-clause metadata (length, flags, literal-range start) lives in
//! [`ClauseArena::meta`]. Literal bodies are concatenated in
//! [`ClauseArena::lits`]. A [`ClauseId`] is `NonZeroU32(slot + 1)` where
//! `slot` indexes `meta`; the slot's [`ClauseMeta`] carries a range into
//! `lits`. Deleted clauses keep their metadata and literal range until the
//! next compaction, which is handled outside this module.
//!
//! Learned-clause LBDs live in a sparse side table [`ClauseArena::lbds`],
//! addressed through a learned index packed into [`ClauseMeta::flags`]. That
//! keeps per-clause metadata to twelve bytes for non-learned clauses, which
//! dominate the arena on industrial instances; only learned clauses pay for
//! an extra four-byte LBD slot.
//!
//! This layout preserves `&[Lit]` access without `unsafe` code (the crate
//! forbids `unsafe_code`). Metadata reads stay out of the propagation hot
//! path, which touches only the literal slice.

use alloc::vec::Vec;
use core::num::NonZeroU32;

use crate::error::{Error, Result};
use crate::types::{ClauseId, Lit};

const FLAG_DELETED: u32 = 1 << 0;
/// Set when a learned clause has been consulted since the last reduction.
/// Reduced clauses protected by this flag bypass deletion in the current
/// pass; the flag is cleared on every survivor at the end of reduction.
const FLAG_USED: u32 = 1 << 1;
/// Shift applied to the learned index before it is stored in
/// [`ClauseMeta::flags`]. A value of zero in the upper bits means the clause
/// is not learned; otherwise the stored value is `learned_index + 1` so that
/// index zero is still representable.
const LEARNED_IDX_SHIFT: u32 = 2;

/// Metadata for a single clause inside [`ClauseArena`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClauseMeta {
    /// Index into `ClauseArena::lits` of the first literal.
    pub(crate) lits_start: u32,
    /// Number of literals in this clause.
    pub(crate) len: u32,
    /// Packed flags. Bit 0 is the deleted flag; the remaining bits hold
    /// `learned_index + 1` for learned clauses (zero means not learned).
    flags: u32,
}

impl ClauseMeta {
    /// Returns `true` if this clause was learned by conflict analysis.
    #[inline]
    #[allow(dead_code, reason = "consumed when inprocessing walks learned clauses")]
    pub(crate) const fn is_learned(self) -> bool {
        self.flags >> LEARNED_IDX_SHIFT != 0
    }

    /// Returns `true` if this clause has been marked for deletion.
    #[inline]
    pub(crate) const fn is_deleted(self) -> bool {
        self.flags & FLAG_DELETED != 0
    }

    /// Returns `true` if this clause has been consulted since the last
    /// reduction pass.
    #[inline]
    const fn is_used(self) -> bool {
        self.flags & FLAG_USED != 0
    }

    /// Returns the learned-clause index for this clause, or `None` when the
    /// clause is not learned.
    #[inline]
    const fn learned_index(self) -> Option<usize> {
        let raw = self.flags >> LEARNED_IDX_SHIFT;
        if raw == 0 {
            None
        } else {
            Some((raw - 1) as usize)
        }
    }
}

/// Flat clause arena.
#[derive(Debug, Default)]
pub(crate) struct ClauseArena {
    meta: Vec<ClauseMeta>,
    lits: Vec<Lit>,
    /// Side table of literal-block distances for learned clauses. Indexed by
    /// the learned-index packed into [`ClauseMeta::flags`] and only populated
    /// for learned clauses.
    lbds: Vec<u32>,
}

impl ClauseArena {
    /// Creates an empty arena.
    pub(crate) const fn new() -> Self {
        Self { meta: Vec::new(), lits: Vec::new(), lbds: Vec::new() }
    }

    /// Returns the number of clauses (including deleted but un-compacted).
    pub(crate) fn num_clauses(&self) -> usize {
        self.meta.len()
    }

    /// Returns the total number of literal words stored across all clauses.
    #[allow(dead_code, reason = "used by inprocessing passes to size scratch buffers")]
    pub(crate) fn num_lit_words(&self) -> usize {
        self.lits.len()
    }

    /// Returns `true` if the arena holds no clauses.
    #[allow(dead_code, reason = "state fixtures and integrity checks call this")]
    pub(crate) fn is_empty(&self) -> bool {
        self.meta.is_empty()
    }

    /// Removes every clause.
    #[allow(dead_code, reason = "reserved for full solver reset path")]
    pub(crate) fn clear(&mut self) {
        self.meta.clear();
        self.lits.clear();
        self.lbds.clear();
    }

    /// Appends a clause and returns its id.
    ///
    /// Returns [`Error::ClauseLimitExceeded`] if the arena has exhausted its
    /// `u32`-sized addressing space.
    pub(crate) fn push(&mut self, body: &[Lit], learned: bool, lbd: u32) -> Result<ClauseId> {
        let len = u32::try_from(body.len()).map_err(|_| Error::ClauseLimitExceeded)?;
        let lits_start =
            u32::try_from(self.lits.len()).map_err(|_| Error::ClauseLimitExceeded)?;
        // Overflow guard on the end of the literal range.
        let _end = lits_start.checked_add(len).ok_or(Error::ClauseLimitExceeded)?;

        let slot = u32::try_from(self.meta.len()).map_err(|_| Error::ClauseLimitExceeded)?;
        let id_raw = slot.checked_add(1).ok_or(Error::ClauseLimitExceeded)?;
        let nz = NonZeroU32::new(id_raw).ok_or(Error::ClauseLimitExceeded)?;

        let flags = if learned {
            let learned_idx = u32::try_from(self.lbds.len())
                .map_err(|_| Error::ClauseLimitExceeded)?;
            let stored = learned_idx
                .checked_add(1)
                .ok_or(Error::ClauseLimitExceeded)?;
            let shifted = stored
                .checked_shl(LEARNED_IDX_SHIFT)
                .ok_or(Error::ClauseLimitExceeded)?;
            self.lbds.push(lbd);
            shifted
        } else {
            0
        };
        self.meta.push(ClauseMeta { lits_start, len, flags });
        self.lits.extend_from_slice(body);
        Ok(ClauseId::from_raw(nz))
    }

    #[inline]
    const fn slot(id: ClauseId) -> usize {
        (id.to_raw().get() - 1) as usize
    }

    /// Returns the metadata for a clause.
    #[inline]
    #[allow(dead_code, reason = "consumed by inprocessing and proof-writer integrations")]
    pub(crate) fn meta(&self, id: ClauseId) -> ClauseMeta {
        self.meta[Self::slot(id)]
    }

    /// Returns the length of a clause in literals.
    #[inline]
    #[allow(dead_code, reason = "inprocessing and proof writers inspect clause length")]
    pub(crate) fn len(&self, id: ClauseId) -> u32 {
        self.meta[Self::slot(id)].len
    }

    /// Returns `true` if the clause was learned.
    #[inline]
    #[allow(dead_code, reason = "consumed by reduceDB and inprocessing paths")]
    pub(crate) fn is_learned(&self, id: ClauseId) -> bool {
        self.meta[Self::slot(id)].is_learned()
    }

    /// Returns `true` if the clause is marked deleted.
    #[inline]
    pub(crate) fn is_deleted(&self, id: ClauseId) -> bool {
        self.meta[Self::slot(id)].is_deleted()
    }

    /// Returns the LBD of a clause. Non-learned clauses report `0`.
    #[inline]
    pub(crate) fn lbd(&self, id: ClauseId) -> u32 {
        self.meta[Self::slot(id)]
            .learned_index()
            .map_or(0, |idx| self.lbds[idx])
    }

    /// Overwrites the LBD of a clause. No-op if the clause is not learned.
    #[inline]
    #[allow(dead_code, reason = "vivification rewrites LBD after shortening a clause")]
    pub(crate) fn set_lbd(&mut self, id: ClauseId, lbd: u32) {
        if let Some(idx) = self.meta[Self::slot(id)].learned_index() {
            self.lbds[idx] = lbd;
        }
    }

    /// Marks a clause deleted. The literal range remains addressable until
    /// the next compaction.
    #[inline]
    pub(crate) fn mark_deleted(&mut self, id: ClauseId) {
        self.meta[Self::slot(id)].flags |= FLAG_DELETED;
    }

    /// Returns `true` if the clause has been consulted since the last
    /// reduction pass.
    #[inline]
    pub(crate) fn used(&self, id: ClauseId) -> bool {
        self.meta[Self::slot(id)].is_used()
    }

    /// Flags the clause as consulted, protecting it from the next
    /// reduction pass regardless of its tier.
    #[inline]
    pub(crate) fn set_used(&mut self, id: ClauseId) {
        self.meta[Self::slot(id)].flags |= FLAG_USED;
    }

    /// Clears the used flag. The reduction pass calls this on every
    /// surviving clause after classification so the next window starts
    /// from a clean slate.
    #[inline]
    pub(crate) fn clear_used(&mut self, id: ClauseId) {
        self.meta[Self::slot(id)].flags &= !FLAG_USED;
    }

    /// Returns the literals of a clause.
    #[inline]
    pub(crate) fn lits(&self, id: ClauseId) -> &[Lit] {
        let m = self.meta[Self::slot(id)];
        let start = m.lits_start as usize;
        let end = start + m.len as usize;
        &self.lits[start..end]
    }

    /// Returns a mutable view of the literals of a clause.
    ///
    /// Used by propagation to swap watched literals into positions 0 and 1.
    #[inline]
    pub(crate) fn lits_mut(&mut self, id: ClauseId) -> &mut [Lit] {
        let m = self.meta[Self::slot(id)];
        let start = m.lits_start as usize;
        let end = start + m.len as usize;
        &mut self.lits[start..end]
    }

    /// Rewrites the arena in place, dropping every clause marked deleted.
    /// Returns a remap table indexed by the old slot; entry `i` is `Some(new_id)`
    /// if the clause survived or `None` if it was dropped. Live clauses keep
    /// their relative order so callers can rewrite external `ClauseId`s
    /// through the table.
    pub(crate) fn compact(&mut self) -> Result<Vec<Option<ClauseId>>> {
        let mut remap: Vec<Option<ClauseId>> = Vec::with_capacity(self.meta.len());
        let mut new_meta: Vec<ClauseMeta> = Vec::with_capacity(self.meta.len());
        let mut new_lits: Vec<Lit> = Vec::with_capacity(self.lits.len());
        let mut new_lbds: Vec<u32> = Vec::with_capacity(self.lbds.len());

        for old_slot in 0..self.meta.len() {
            let meta = self.meta[old_slot];
            if meta.is_deleted() {
                remap.push(None);
                continue;
            }
            let start = meta.lits_start as usize;
            let end = start + meta.len as usize;
            let lits_start =
                u32::try_from(new_lits.len()).map_err(|_| Error::ClauseLimitExceeded)?;
            new_lits.extend_from_slice(&self.lits[start..end]);
            let new_slot =
                u32::try_from(new_meta.len()).map_err(|_| Error::ClauseLimitExceeded)?;
            let id_raw = new_slot.checked_add(1).ok_or(Error::ClauseLimitExceeded)?;
            let nz = NonZeroU32::new(id_raw).ok_or(Error::ClauseLimitExceeded)?;
            let new_flags = if let Some(old_idx) = meta.learned_index() {
                let old_lbd = self.lbds[old_idx];
                let new_idx = u32::try_from(new_lbds.len())
                    .map_err(|_| Error::ClauseLimitExceeded)?;
                let stored = new_idx.checked_add(1).ok_or(Error::ClauseLimitExceeded)?;
                let shifted = stored
                    .checked_shl(LEARNED_IDX_SHIFT)
                    .ok_or(Error::ClauseLimitExceeded)?;
                new_lbds.push(old_lbd);
                shifted
            } else {
                0
            };
            new_meta.push(ClauseMeta { lits_start, len: meta.len, flags: new_flags });
            remap.push(Some(ClauseId::from_raw(nz)));
        }

        self.meta = new_meta;
        self.lits = new_lits;
        self.lbds = new_lbds;
        Ok(remap)
    }

    /// Returns the zero-based slot index behind `id`.
    #[inline]
    pub(crate) const fn slot_of(id: ClauseId) -> usize {
        Self::slot(id)
    }
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
    fn push_then_read() {
        let mut a = ClauseArena::new();
        let lits = [v(1).pos(), v(2).neg(), v(3).pos()];
        let id = a.push(&lits, false, 0).unwrap();
        assert_eq!(a.lits(id), &lits);
        assert_eq!(a.len(id), 3);
        assert!(!a.is_learned(id));
        assert!(!a.is_deleted(id));
    }

    #[test]
    fn learned_flag_round_trip() {
        let mut a = ClauseArena::new();
        let id = a.push(&[v(1).pos(), v(2).neg()], true, 5).unwrap();
        assert!(a.is_learned(id));
        assert_eq!(a.lbd(id), 5);
    }

    #[test]
    fn set_lbd_updates() {
        let mut a = ClauseArena::new();
        let id = a.push(&[v(1).pos(), v(2).neg(), v(3).neg()], true, 7).unwrap();
        a.set_lbd(id, 2);
        assert_eq!(a.lbd(id), 2);
    }

    #[test]
    fn mark_deleted_sticks() {
        let mut a = ClauseArena::new();
        let id = a.push(&[v(1).pos(), v(2).neg(), v(3).pos()], true, 4).unwrap();
        assert!(!a.is_deleted(id));
        a.mark_deleted(id);
        assert!(a.is_deleted(id));
        assert_eq!(a.lits(id).len(), 3, "deleted clauses keep their body until compaction");
    }

    #[test]
    fn distinct_ids_distinct_bodies() {
        let mut a = ClauseArena::new();
        let id1 = a.push(&[v(1).pos(), v(2).neg(), v(3).pos()], false, 0).unwrap();
        let id2 = a.push(&[v(4).pos(), v(5).pos(), v(6).neg()], false, 0).unwrap();
        assert_ne!(id1, id2);
        assert_eq!(a.lits(id1)[0], v(1).pos());
        assert_eq!(a.lits(id2)[0], v(4).pos());
    }

    #[test]
    fn lits_mut_swaps_positions() {
        let mut a = ClauseArena::new();
        let id = a.push(&[v(1).pos(), v(2).neg(), v(3).pos()], false, 0).unwrap();
        a.lits_mut(id).swap(0, 1);
        assert_eq!(a.lits(id)[0], v(2).neg());
        assert_eq!(a.lits(id)[1], v(1).pos());
    }

    #[test]
    fn clause_id_niche_holds() {
        let mut a = ClauseArena::new();
        let id = a.push(&[v(1).pos(), v(2).pos(), v(3).pos()], false, 0).unwrap();
        assert_eq!(core::mem::size_of::<Option<ClauseId>>(), core::mem::size_of::<ClauseId>());
        // `slot(id) == 0` for the first clause.
        assert_eq!(id.to_raw().get(), 1);
    }
}
