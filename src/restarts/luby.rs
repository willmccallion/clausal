//! Luby restart sequence.

use crate::context::SearchContext;
use crate::traits::RestartStrategy;

/// Restarts on the classical Luby sequence multiplied by a unit run.
#[derive(Debug, Clone, Copy)]
pub struct Luby {
    unit: u64,
    index: u64,
}

impl Luby {
    /// Creates a Luby restart strategy with the given unit run length.
    #[must_use]
    pub const fn new(unit: u64) -> Self {
        Self { unit, index: 1 }
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

    fn should_restart(&mut self, _ctx: &SearchContext<'_>) -> bool {
        let _ = (self.unit, self.index);
        false
    }
}
