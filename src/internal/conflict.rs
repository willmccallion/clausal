//! Conflict descriptor passed from propagation to analysis.

use crate::types::{ClauseId, Lit};

/// A conflict detected during boolean constraint propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Conflict {
    /// A long clause was falsified.
    LongClause(ClauseId),
    /// A binary clause was falsified; both literals are kept inline.
    Binary([Lit; 2]),
}
