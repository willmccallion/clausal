//! Flat clause arena backed by struct-of-arrays storage.
//!
//! Per-clause metadata (length, flags, LBD, literal-range start) lives in
//! [`ClauseArena::meta`]. Literal bodies are concatenated in
//! [`ClauseArena::lits`]. A [`ClauseId`] is `NonZeroU32(slot + 1)` where
//! `slot` indexes `meta`; the slot's [`ClauseMeta`] carries a range into
//! `lits`. Deleted clauses keep their metadata and literal range until the
//! next compaction, which is handled outside this module.
//!
//! This layout preserves `&[Lit]` access without `unsafe` code (the crate
//! forbids `unsafe_code`) at the cost of sixteen bytes of metadata per
//! clause versus the packed 8-byte header used by some reference solvers.
//! The extra bytes are out of line from the propagation hot path, which
//! reads the literal slice directly.

use alloc::vec::Vec;
use core::num::NonZeroU32;

use crate::error::{Error, Result};
use crate::types::{ClauseId, Lit};

const FLAG_LEARNED: u32 = 1 << 0;
const FLAG_DELETED: u32 = 1 << 1;

/// Metadata for a single clause inside [`ClauseArena`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClauseMeta {
    /// Index into `ClauseArena::lits` of the first literal.
    pub(crate) lits_start: u32,
    /// Number of literals in this clause.
    pub(crate) len: u32,
    /// Literal-block distance (only meaningful for learned clauses).
    pub(crate) lbd: u32,
    flags: u32,
}

impl ClauseMeta {
    /// Returns `true` if this clause was learned by conflict analysis.
    #[inline]
    pub(crate) const fn is_learned(self) -> bool {
        self.flags & FLAG_LEARNED != 0
    }

    /// Returns `true` if this clause has been marked for deletion.
    #[inline]
    pub(crate) const fn is_deleted(self) -> bool {
        self.flags & FLAG_DELETED != 0
    }
}

/// Flat clause arena.
#[derive(Debug, Default)]
pub(crate) struct ClauseArena {
    meta: Vec<ClauseMeta>,
    lits: Vec<Lit>,
}

impl ClauseArena {
    /// Creates an empty arena.
    pub(crate) const fn new() -> Self {
        Self { meta: Vec::new(), lits: Vec::new() }
    }

    /// Returns the number of clauses (including deleted but un-compacted).
    pub(crate) fn num_clauses(&self) -> usize {
        self.meta.len()
    }

    /// Returns the total number of literal words stored across all clauses.
    pub(crate) fn num_lit_words(&self) -> usize {
        self.lits.len()
    }

    /// Returns `true` if the arena holds no clauses.
    pub(crate) fn is_empty(&self) -> bool {
        self.meta.is_empty()
    }

    /// Removes every clause.
    pub(crate) fn clear(&mut self) {
        self.meta.clear();
        self.lits.clear();
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

        let flags = if learned { FLAG_LEARNED } else { 0 };
        self.meta.push(ClauseMeta { lits_start, len, lbd, flags });
        self.lits.extend_from_slice(body);
        Ok(ClauseId::from_raw(nz))
    }

    #[inline]
    fn slot(id: ClauseId) -> usize {
        // `ClauseId` stores `slot + 1` in its nonzero inner value.
        (id.to_raw().get() - 1) as usize
    }

    /// Returns the metadata for a clause.
    #[inline]
    pub(crate) fn meta(&self, id: ClauseId) -> ClauseMeta {
        self.meta[Self::slot(id)]
    }

    /// Returns the length of a clause in literals.
    #[inline]
    pub(crate) fn len(&self, id: ClauseId) -> u32 {
        self.meta[Self::slot(id)].len
    }

    /// Returns `true` if the clause was learned.
    #[inline]
    pub(crate) fn is_learned(&self, id: ClauseId) -> bool {
        self.meta[Self::slot(id)].is_learned()
    }

    /// Returns `true` if the clause is marked deleted.
    #[inline]
    pub(crate) fn is_deleted(&self, id: ClauseId) -> bool {
        self.meta[Self::slot(id)].is_deleted()
    }

    /// Returns the LBD of a clause.
    #[inline]
    pub(crate) fn lbd(&self, id: ClauseId) -> u32 {
        self.meta[Self::slot(id)].lbd
    }

    /// Overwrites the LBD of a clause.
    #[inline]
    pub(crate) fn set_lbd(&mut self, id: ClauseId, lbd: u32) {
        self.meta[Self::slot(id)].lbd = lbd;
    }

    /// Marks a clause deleted. The literal range remains addressable until
    /// the next compaction.
    #[inline]
    pub(crate) fn mark_deleted(&mut self, id: ClauseId) {
        self.meta[Self::slot(id)].flags |= FLAG_DELETED;
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
            new_meta.push(ClauseMeta { lits_start, ..meta });
            remap.push(Some(ClauseId::from_raw(nz)));
        }

        self.meta = new_meta;
        self.lits = new_lits;
        Ok(remap)
    }

    /// Returns the zero-based slot index behind `id`.
    #[inline]
    pub(crate) fn slot_of(id: ClauseId) -> usize {
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
