//! Flat clause arena.
//!
//! A single `Vec<u32>` holds every clause back-to-back, each prefixed by a
//! two-word header carrying flags and LBD. [`ClauseId`] is an index into
//! this arena offset by one so zero remains a niche for `Option<ClauseId>`.

use alloc::vec::Vec;

use crate::types::ClauseId;

/// Header flag: clause was learned by conflict analysis.
pub(crate) const FLAG_LEARNED: u32 = 1 << 0;
/// Header flag: clause is marked for deletion on the next sweep.
pub(crate) const FLAG_DELETED: u32 = 1 << 1;
/// Header flag: clause has been used in a recent conflict.
pub(crate) const FLAG_USED: u32 = 1 << 2;

/// Flat-arena clause store.
#[derive(Debug, Default)]
pub(crate) struct ClauseArena {
    words: Vec<u32>,
}

impl ClauseArena {
    pub(crate) const fn new() -> Self {
        Self { words: Vec::new() }
    }

    pub(crate) fn len_words(&self) -> usize {
        self.words.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.words.clear();
    }

    /// Returns the raw backing buffer. Crate-private; callers navigate via
    /// [`ClauseId`]-derived offsets.
    pub(crate) fn words(&self) -> &[u32] {
        &self.words
    }

    pub(crate) fn words_mut(&mut self) -> &mut [u32] {
        &mut self.words
    }

    pub(crate) fn resolve(&self, _id: ClauseId) -> ClauseSlot<'_> {
        ClauseSlot { arena: self }
    }
}

/// Borrowed handle into one clause inside the arena.
#[derive(Debug)]
pub(crate) struct ClauseSlot<'a> {
    #[allow(dead_code)]
    arena: &'a ClauseArena,
}
