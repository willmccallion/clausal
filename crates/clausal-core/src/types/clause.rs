//! [`Clause`]: a disjunction of literals, and [`ClauseId`]: an opaque solver handle.

use alloc::vec::Vec;
use core::num::NonZeroU32;
use core::slice;

use super::Lit;

/// An opaque handle to a clause stored inside a solver.
///
/// `ClauseId` values are only meaningful to the solver that produced them.
/// The inner `NonZeroU32` gives `Option<ClauseId>` the same size as
/// `ClauseId` via niche optimisation.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
#[repr(transparent)]
pub struct ClauseId(NonZeroU32);

impl ClauseId {
    /// Wraps a raw nonzero id. Solver-internal use only.
    #[inline]
    #[must_use]
    pub(crate) const fn from_raw(raw: NonZeroU32) -> Self {
        Self(raw)
    }

    /// Returns the underlying raw value. Solver-internal use only.
    #[inline]
    #[must_use]
    pub(crate) const fn to_raw(self) -> NonZeroU32 {
        self.0
    }
}

/// A disjunction of literals.
///
/// `Clause` owns its literals in a `Vec<Lit>`. Clauses are plain data
/// containers: no deduplication, no tautology check, no sorting. Those
/// semantics belong to the solver that ingests the clause.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Clause {
    lits: Vec<Lit>,
}

impl Clause {
    /// Creates an empty clause.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self { lits: Vec::new() }
    }

    /// Creates an empty clause with the given capacity.
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self { lits: Vec::with_capacity(capacity) }
    }

    /// Creates a clause from an iterator of literals, preserving order.
    #[inline]
    #[must_use]
    pub fn from_lits<I: IntoIterator<Item = Lit>>(lits: I) -> Self {
        Self { lits: lits.into_iter().collect() }
    }

    /// Appends a literal to the end of the clause.
    #[inline]
    pub fn push(&mut self, lit: Lit) {
        self.lits.push(lit);
    }

    /// Returns the number of literals in the clause.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.lits.len()
    }

    /// Returns `true` if the clause contains no literals.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lits.is_empty()
    }

    /// Returns an iterator over the clause's literals in insertion order.
    #[inline]
    pub fn iter(&self) -> slice::Iter<'_, Lit> {
        self.lits.iter()
    }

    /// Returns the clause's literals as a slice.
    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[Lit] {
        &self.lits
    }
}

impl<'a> IntoIterator for &'a Clause {
    type Item = &'a Lit;
    type IntoIter = slice::Iter<'a, Lit>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl FromIterator<Lit> for Clause {
    fn from_iter<I: IntoIterator<Item = Lit>>(iter: I) -> Self {
        Self::from_lits(iter)
    }
}

impl Extend<Lit> for Clause {
    fn extend<I: IntoIterator<Item = Lit>>(&mut self, iter: I) {
        self.lits.extend(iter);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::types::Var;

    fn lits() -> [Lit; 3] {
        let v = |n| Var::new(n).unwrap();
        [v(1).pos(), v(2).neg(), v(3).pos()]
    }

    #[test]
    fn new_is_empty() {
        let c = Clause::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn from_lits_preserves_order() {
        let input = lits();
        let c = Clause::from_lits(input);
        assert_eq!(c.as_slice(), &input);
    }

    #[test]
    fn push_grows() {
        let mut c = Clause::new();
        for lit in lits() {
            c.push(lit);
        }
        assert_eq!(c.len(), 3);
        assert_eq!(c.as_slice(), &lits());
    }

    #[test]
    fn iter_matches_slice() {
        let c = Clause::from_lits(lits());
        let collected: Vec<Lit> = c.iter().copied().collect();
        assert_eq!(collected.as_slice(), c.as_slice());
    }

    #[test]
    fn extend_appends() {
        let mut c = Clause::from_lits([lits()[0]]);
        c.extend([lits()[1], lits()[2]]);
        assert_eq!(c.as_slice(), &lits());
    }
}
