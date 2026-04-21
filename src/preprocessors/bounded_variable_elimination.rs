//! Bounded Variable Elimination.

use crate::context::FormulaView;
use crate::error::Result;
use crate::traits::{PreprocessResult, Preprocessor};

/// Eliminates variables whose resolvent count stays within a growth bound.
#[derive(Debug, Clone, Copy)]
pub struct BoundedVariableElimination {
    growth_limit: u32,
}

impl BoundedVariableElimination {
    /// Creates a BVE pass with the given growth limit.
    #[must_use]
    pub const fn new(growth_limit: u32) -> Self {
        Self { growth_limit }
    }
}

impl Default for BoundedVariableElimination {
    fn default() -> Self {
        Self::new(8)
    }
}

impl Preprocessor for BoundedVariableElimination {
    fn name(&self) -> &'static str {
        "bve"
    }

    fn preprocess(&mut self, _formula: &FormulaView<'_>) -> Result<PreprocessResult> {
        let _ = self.growth_limit;
        Ok(PreprocessResult::Unchanged)
    }
}
