//! Two-watched-literal data structures.

use crate::types::{ClauseId, Lit};

/// A watcher on a long clause.
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
