//! [`Lit`]: a variable paired with a polarity, and [`Polarity`] itself.

use core::num::NonZeroU32;
use core::ops::Not;

use super::Var;

/// Whether a literal asserts a variable is true or false.
///
/// A plain `bool` is a footgun in literal-heavy code; `Polarity` is the
/// public-facing sign type throughout the solver.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug, Default)]
pub enum Polarity {
    /// The literal asserts its variable is true.
    #[default]
    Positive,
    /// The literal asserts its variable is false.
    Negative,
}

impl Polarity {
    /// Returns the low-bit encoding of this polarity, where `0` is positive
    /// and `1` is negative. This matches the encoding used inside [`Lit`].
    #[inline]
    #[must_use]
    pub const fn as_bit(self) -> u32 {
        match self {
            Self::Positive => 0,
            Self::Negative => 1,
        }
    }

    /// Decodes a polarity from a low-bit encoding. Only bit 0 is inspected.
    #[inline]
    #[must_use]
    pub const fn from_bit(bit: u32) -> Self {
        if bit & 1 == 0 { Self::Positive } else { Self::Negative }
    }
}

impl Not for Polarity {
    type Output = Self;

    #[inline]
    fn not(self) -> Self {
        match self {
            Self::Positive => Self::Negative,
            Self::Negative => Self::Positive,
        }
    }
}

/// A literal: a variable together with a polarity.
///
/// `Lit` is encoded internally as `(var.to_raw() << 1) | polarity_bit`,
/// where the polarity bit is `0` for positive and `1` for negative. This
/// matches the encoding used by most high-performance SAT solvers and means
/// [`Lit::index`] returns a direct offset into literal-indexed tables such
/// as watchers and value arrays.
///
/// The inner `NonZeroU32` guarantees `Option<Lit>` has the same size as
/// `Lit`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
#[repr(transparent)]
pub struct Lit(NonZeroU32);

impl Lit {
    /// Builds a literal from a variable and polarity.
    ///
    /// # Panics
    ///
    /// Never in practice: the encoding `(var.to_raw() << 1) | pol_bit` is
    /// always `>= 2` because `var.to_raw() >= 1`. The panic is unreachable
    /// and exists only to keep this function `const`.
    #[inline]
    #[must_use]
    pub const fn new(var: Var, pol: Polarity) -> Self {
        let raw = (var.to_raw() << 1) | pol.as_bit();
        match NonZeroU32::new(raw) {
            Some(n) => Self(n),
            None => panic!("internal invariant: Lit encoding produced zero"),
        }
    }

    /// Parses a literal from its signed DIMACS representation.
    ///
    /// Positive integers produce positive literals, negative integers
    /// produce negative literals, and `0` is rejected.
    #[inline]
    #[must_use]
    pub const fn from_dimacs(n: i32) -> Option<Self> {
        if n == 0 {
            return None;
        }
        let raw = n.unsigned_abs();
        let pol = if n > 0 { Polarity::Positive } else { Polarity::Negative };
        match Var::new(raw) {
            Some(var) => Some(Self::new(var, pol)),
            None => None,
        }
    }

    /// Returns the DIMACS-signed representation of this literal.
    #[inline]
    #[must_use]
    pub const fn to_dimacs(self) -> i32 {
        // `Var::MAX_RAW == i32::MAX as u32`, so the cast never wraps.
        #[allow(clippy::cast_possible_wrap)]
        let magnitude = self.var().to_raw() as i32;
        match self.polarity() {
            Polarity::Positive => magnitude,
            Polarity::Negative => -magnitude,
        }
    }

    /// Returns the variable this literal refers to.
    ///
    /// # Panics
    ///
    /// Never in practice: the encoding guarantees the upper bits decode to
    /// a nonzero variable. The panic is unreachable and exists only to keep
    /// this function `const`.
    #[inline]
    #[must_use]
    pub const fn var(self) -> Var {
        let raw = self.0.get() >> 1;
        match Var::new(raw) {
            Some(v) => v,
            None => panic!("internal invariant: Lit decodes to a zero variable"),
        }
    }

    /// Returns the polarity of this literal.
    #[inline]
    #[must_use]
    pub const fn polarity(self) -> Polarity {
        Polarity::from_bit(self.0.get())
    }

    /// Returns `true` if this literal is positive.
    #[inline]
    #[must_use]
    pub const fn is_positive(self) -> bool {
        matches!(self.polarity(), Polarity::Positive)
    }

    /// Returns `true` if this literal is negative.
    #[inline]
    #[must_use]
    pub const fn is_negative(self) -> bool {
        matches!(self.polarity(), Polarity::Negative)
    }

    /// Returns the 0-based literal index suitable for literal-indexed tables.
    ///
    /// Contract: `lit.index() == lit.var().index() * 2 + lit.polarity().as_bit() as usize`.
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        // `Lit`'s encoding starts at 2 (Var 1 positive), so subtract 2.
        (self.0.get() - 2) as usize
    }

    /// Returns the raw nonzero encoding: `(var.to_raw() << 1) | polarity_bit`.
    ///
    /// Solver-internal use only.
    #[inline]
    #[must_use]
    #[allow(dead_code, reason = "arena round-trips Lit values through u32; retained for future inprocessing")]
    pub(crate) const fn to_raw(self) -> u32 {
        self.0.get()
    }

    /// Rebuilds a literal from a raw word produced by [`Self::to_raw`].
    ///
    /// Returns `None` if `raw` does not encode a valid literal (values below
    /// `2` or with a variable portion outside `1..=Var::MAX_RAW`).
    #[inline]
    #[must_use]
    #[allow(dead_code, reason = "arena round-trips Lit values through u32; retained for future inprocessing")]
    pub(crate) const fn from_raw(raw: u32) -> Option<Self> {
        if raw < 2 {
            return None;
        }
        let var_raw = raw >> 1;
        if Var::new(var_raw).is_none() {
            return None;
        }
        match NonZeroU32::new(raw) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }
}

impl Not for Lit {
    type Output = Self;

    #[inline]
    fn not(self) -> Self {
        let flipped = self.0.get() ^ 1;
        NonZeroU32::new(flipped).map_or_else(
            || panic!("internal invariant: Lit negation produced zero"),
            Self,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    #[test]
    fn dimacs_round_trip() {
        for n in [1, -1, 7, -7, 1024, -1024, i32::MAX, -i32::MAX] {
            let lit = Lit::from_dimacs(n).unwrap();
            assert_eq!(lit.to_dimacs(), n);
        }
    }

    #[test]
    fn zero_dimacs_rejected() {
        assert!(Lit::from_dimacs(0).is_none());
    }

    #[test]
    fn i32_min_rejected() {
        // `i32::MIN.unsigned_abs()` would wrap; Var::new rejects it via MAX_RAW.
        assert!(Lit::from_dimacs(i32::MIN).is_none());
    }

    #[test]
    fn negation_is_involution() {
        let lit = v(42).pos();
        assert_eq!(!!lit, lit);
    }

    #[test]
    fn negation_preserves_variable() {
        let lit = v(42).pos();
        assert_eq!((!lit).var(), lit.var());
    }

    #[test]
    fn negation_flips_polarity() {
        let lit = v(42).pos();
        assert_eq!((!lit).polarity(), !lit.polarity());
    }

    #[test]
    fn new_respects_polarity() {
        assert_eq!(Lit::new(v(3), Polarity::Positive).polarity(), Polarity::Positive);
        assert_eq!(Lit::new(v(3), Polarity::Negative).polarity(), Polarity::Negative);
    }

    #[test]
    fn index_contract() {
        for raw in [1u32, 2, 99, 1000, Var::MAX_RAW] {
            let var = v(raw);
            assert_eq!(var.pos().index(), var.index() * 2);
            assert_eq!(var.neg().index(), var.index() * 2 + 1);
        }
    }

    #[test]
    fn polarity_not() {
        assert_eq!(!Polarity::Positive, Polarity::Negative);
        assert_eq!(!Polarity::Negative, Polarity::Positive);
    }
}
