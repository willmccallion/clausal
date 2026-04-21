//! [`DecisionLevel`]: a typed wrapper over the solver's current decision depth.

/// The solver's decision depth.
///
/// Level `0` is ground: a literal assigned at level 0 holds in every model of
/// the original formula. Each decision opens a new level.
///
/// Wrapping a raw `u32` keeps decision levels distinct from conflict counts,
/// trail indexes, and other `u32`-shaped quantities flowing through the
/// search context.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug, Default)]
#[repr(transparent)]
pub struct DecisionLevel(u32);

impl DecisionLevel {
    /// The ground level (no decisions active).
    pub const GROUND: Self = Self(0);

    /// Wraps a raw `u32`.
    #[inline]
    #[must_use]
    pub const fn new(level: u32) -> Self {
        Self(level)
    }

    /// Returns the underlying raw value.
    #[inline]
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Returns `true` if this is the ground level.
    #[inline]
    #[must_use]
    pub const fn is_ground(self) -> bool {
        self.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_is_zero() {
        assert_eq!(DecisionLevel::GROUND.get(), 0);
        assert!(DecisionLevel::GROUND.is_ground());
    }

    #[test]
    fn round_trip() {
        for n in [0u32, 1, 42, u32::MAX] {
            assert_eq!(DecisionLevel::new(n).get(), n);
        }
    }

    #[test]
    fn default_is_ground() {
        assert_eq!(DecisionLevel::default(), DecisionLevel::GROUND);
    }
}
