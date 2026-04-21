//! Trail reason tag.
//!
//! A single word per trail entry distinguishes between a decision, a
//! long-clause propagation, and a binary-clause propagation.

use crate::types::{ClauseId, Lit};

/// Reason a literal appears on the trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The literal was chosen as a decision at its level.
    Decision,
    /// Forced by propagation through a long clause.
    LongClause(ClauseId),
    /// Forced by propagation through a binary clause; the partner literal
    /// is the other watched lit of the binary pair.
    Binary(Lit),
}
