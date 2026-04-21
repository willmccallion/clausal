//! Trail reason tag.
//!
//! A single word per trail entry distinguishes between a decision, a
//! long-clause propagation, and a binary-clause propagation. Binary
//! reasons carry the partner literal inline so analysis never has to
//! visit the arena for two-literal clauses.

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

impl Reason {
    /// Constructs a decision reason.
    #[inline]
    pub(crate) const fn decision() -> Self {
        Self::Decision
    }

    /// Constructs a long-clause propagation reason.
    #[inline]
    pub(crate) const fn long(id: ClauseId) -> Self {
        Self::LongClause(id)
    }

    /// Constructs a binary-clause propagation reason with the partner literal.
    #[inline]
    pub(crate) const fn binary(partner: Lit) -> Self {
        Self::Binary(partner)
    }

    /// Returns `true` if this reason indicates a free choice (decision).
    #[inline]
    pub(crate) const fn is_decision(self) -> bool {
        matches!(self, Self::Decision)
    }

    /// Returns `true` if this reason is a binary propagation.
    #[inline]
    #[allow(dead_code, reason = "inprocessing consults reason kind while walking the trail")]
    pub(crate) const fn is_binary(self) -> bool {
        matches!(self, Self::Binary(_))
    }

    /// Returns the long-clause id if this reason is a long-clause propagation.
    #[inline]
    pub(crate) const fn as_long(self) -> Option<ClauseId> {
        match self {
            Self::LongClause(id) => Some(id),
            _ => None,
        }
    }

    /// Returns the partner literal if this reason is a binary propagation.
    #[inline]
    #[allow(dead_code, reason = "conflict minimization needs the partner literal")]
    pub(crate) const fn as_binary(self) -> Option<Lit> {
        match self {
            Self::Binary(lit) => Some(lit),
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::types::Var;
    use core::num::NonZeroU32;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    fn cid(n: u32) -> ClauseId {
        ClauseId::from_raw(NonZeroU32::new(n).unwrap())
    }

    #[test]
    fn decision_predicate() {
        let r = Reason::decision();
        assert!(r.is_decision());
        assert!(!r.is_binary());
        assert!(r.as_long().is_none());
        assert!(r.as_binary().is_none());
    }

    #[test]
    fn long_projection() {
        let r = Reason::long(cid(7));
        assert!(!r.is_decision());
        assert!(!r.is_binary());
        assert_eq!(r.as_long(), Some(cid(7)));
        assert!(r.as_binary().is_none());
    }

    #[test]
    fn binary_projection() {
        let r = Reason::binary(v(3).neg());
        assert!(!r.is_decision());
        assert!(r.is_binary());
        assert_eq!(r.as_binary(), Some(v(3).neg()));
        assert!(r.as_long().is_none());
    }
}
