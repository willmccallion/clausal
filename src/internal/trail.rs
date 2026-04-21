//! Assignment trail and decision-level index.
//!
//! The trail records every assigned literal in chronological order.
//! `trail_lim` indexes the first literal of each decision level so
//! backtracking to level `L` is an O(1) truncation.

use alloc::vec::Vec;

use crate::types::{DecisionLevel, Lit};

/// Chronological stack of assigned literals with per-level boundaries.
#[derive(Debug, Default)]
pub(crate) struct Trail {
    lits: Vec<Lit>,
    level_starts: Vec<u32>,
}

impl Trail {
    pub(crate) const fn new() -> Self {
        Self { lits: Vec::new(), level_starts: Vec::new() }
    }

    pub(crate) fn len(&self) -> usize {
        self.lits.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lits.is_empty()
    }

    pub(crate) fn current_level(&self) -> DecisionLevel {
        #[allow(clippy::cast_possible_truncation)]
        let raw = self.level_starts.len() as u32;
        DecisionLevel::new(raw)
    }

    pub(crate) fn lits(&self) -> &[Lit] {
        &self.lits
    }

    pub(crate) fn level_starts(&self) -> &[u32] {
        &self.level_starts
    }

    pub(crate) fn clear(&mut self) {
        self.lits.clear();
        self.level_starts.clear();
    }
}
