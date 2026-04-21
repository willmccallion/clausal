//! [`Var`]: an opaque handle to a Boolean variable in a SAT formula.

use core::num::NonZeroU32;

use super::{Lit, Polarity};

/// An opaque handle to a Boolean variable in a SAT formula.
///
/// Variables are 1-indexed externally (matching DIMACS) and represented
/// internally by a `NonZeroU32`, which gives `Option<Var>` the same size as
/// `Var` via niche optimisation.
///
/// The valid range is `1..=Var::MAX_RAW`. This upper bound reserves the top
/// bit of the underlying `u32` for the polarity tag used by [`Lit`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
#[repr(transparent)]
pub struct Var(NonZeroU32);

impl Var {
    /// The largest raw value a [`Var`] can take.
    ///
    /// One bit of the underlying `u32` is reserved by [`Lit`] for polarity,
    /// so `Var::MAX_RAW == u32::MAX >> 1`.
    pub const MAX_RAW: u32 = u32::MAX >> 1;

    /// Creates a variable from a 1-indexed raw value.
    ///
    /// Returns `None` if `raw` is `0` or greater than [`Self::MAX_RAW`].
    #[inline]
    #[must_use]
    pub const fn new(raw: u32) -> Option<Self> {
        if raw == 0 || raw > Self::MAX_RAW {
            return None;
        }
        match NonZeroU32::new(raw) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// Returns the 0-based index suitable for direct array lookup.
    ///
    /// Contract: for any variable `v`, `v.pos().index() == v.index() * 2`
    /// and `v.neg().index() == v.index() * 2 + 1`.
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }

    /// Returns the 1-based raw value used by DIMACS.
    #[inline]
    #[must_use]
    pub const fn to_raw(self) -> u32 {
        self.0.get()
    }

    /// Returns the positive literal for this variable.
    #[inline]
    #[must_use]
    pub const fn pos(self) -> Lit {
        Lit::new(self, Polarity::Positive)
    }

    /// Returns the negative literal for this variable.
    #[inline]
    #[must_use]
    pub const fn neg(self) -> Lit {
        Lit::new(self, Polarity::Negative)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn zero_rejected() {
        assert!(Var::new(0).is_none());
    }

    #[test]
    fn overlarge_rejected() {
        assert!(Var::new(u32::MAX).is_none());
        assert!(Var::new(Var::MAX_RAW + 1).is_none());
    }

    #[test]
    fn max_raw_accepted() {
        assert!(Var::new(Var::MAX_RAW).is_some());
    }

    #[test]
    fn index_is_zero_based() {
        assert_eq!(Var::new(1).unwrap().index(), 0);
        assert_eq!(Var::new(42).unwrap().index(), 41);
    }

    #[test]
    fn distinct_vars_are_distinct() {
        let a = Var::new(1).unwrap();
        let b = Var::new(2).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn pos_neg_round_trip_var() {
        let v = Var::new(7).unwrap();
        assert_eq!(v.pos().var(), v);
        assert_eq!(v.neg().var(), v);
    }
}
