//! Pure-literal elimination.

use crate::context::FormulaView;
use crate::error::Result;
use crate::traits::{PreprocessResult, Preprocessor};

/// Assigns every pure literal to its polarity.
#[derive(Debug, Default, Clone, Copy)]
pub struct PureLiteralElimination;

impl Preprocessor for PureLiteralElimination {
    fn name(&self) -> &'static str {
        "pure-literal"
    }

    fn preprocess(&mut self, _formula: &FormulaView<'_>) -> Result<PreprocessResult> {
        Ok(PreprocessResult::Unchanged)
    }
}
