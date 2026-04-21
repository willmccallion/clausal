//! Null restart strategy.

use crate::context::SearchContext;
use crate::traits::RestartStrategy;

/// Never restarts.
#[derive(Debug, Default, Clone, Copy)]
pub struct Never;

impl RestartStrategy for Never {
    fn name(&self) -> &'static str {
        "never"
    }

    fn should_restart(&mut self, _ctx: &SearchContext<'_>) -> bool {
        false
    }
}
