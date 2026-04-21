//! Luby restart sequence.

use crate::context::SearchContext;
use crate::traits::RestartStrategy;

/// Restarts on the classical Luby sequence multiplied by a unit run.
///
/// The sequence `1, 1, 2, 1, 1, 2, 4, 1, 1, 2, 1, 1, 2, 4, 8, ...` sets the
/// next restart threshold; the unit scales every entry.
#[derive(Debug, Clone, Copy)]
pub struct Luby {
    unit: u64,
    restarts: u64,
    conflicts_at_last_restart: u64,
}

impl Luby {
    /// Creates a Luby restart strategy with the given unit run length.
    #[must_use]
    pub const fn new(unit: u64) -> Self {
        Self { unit, restarts: 0, conflicts_at_last_restart: 0 }
    }

    /// Returns the `i`-th Luby number (1-indexed).
    ///
    /// Follows Knuth's iterative formulation: finds the outermost
    /// "complete" sub-sequence containing index `i`, then descends.
    #[must_use]
    pub const fn luby(i: u64) -> u64 {
        if i == 0 {
            return 0;
        }
        let mut index = i - 1;
        let mut size: u64 = 1;
        let mut seq: u32 = 0;
        while size < index + 1 {
            seq += 1;
            size = 2 * size + 1;
        }
        while size - 1 != index {
            size = (size - 1) / 2;
            seq -= 1;
            index %= size;
        }
        1u64 << seq
    }
}

impl Default for Luby {
    fn default() -> Self {
        Self::new(100)
    }
}

impl RestartStrategy for Luby {
    fn name(&self) -> &'static str {
        "luby"
    }

    fn should_restart(&mut self, ctx: &SearchContext<'_>) -> bool {
        let since = ctx.conflicts().saturating_sub(self.conflicts_at_last_restart);
        let threshold = Self::luby(self.restarts.saturating_add(1)).saturating_mul(self.unit);
        since >= threshold
    }

    fn on_restart(&mut self, ctx: &SearchContext<'_>) {
        self.restarts = self.restarts.saturating_add(1);
        self.conflicts_at_last_restart = ctx.conflicts();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luby_sequence_matches_reference() {
        let expected = [1u64, 1, 2, 1, 1, 2, 4, 1, 1, 2, 1, 1, 2, 4, 8];
        for (i, &want) in expected.iter().enumerate() {
            let Ok(idx) = u64::try_from(i) else { continue };
            let idx = idx + 1;
            assert_eq!(Luby::luby(idx), want, "luby({idx})");
        }
    }

    #[test]
    fn name_is_luby() {
        let l = Luby::default();
        assert_eq!(l.name(), "luby");
    }
}
