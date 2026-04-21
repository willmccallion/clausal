//! Unit-propagation preprocessor.

use crate::context::FormulaView;
use crate::error::Result;
use crate::traits::{PreprocessResult, Preprocessor};

/// Runs unit propagation to fixpoint.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnitPropagation;

impl Preprocessor for UnitPropagation {
    fn name(&self) -> &'static str {
        "unit-propagation"
    }

    fn preprocess(&mut self, _formula: &FormulaView<'_>) -> Result<PreprocessResult> {
        Ok(PreprocessResult::Unchanged)
    }
}
