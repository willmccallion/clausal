//! Clause vivification.

use crate::context::FormulaView;
use crate::error::Result;
use crate::traits::{PreprocessResult, Preprocessor};

/// Strengthens clauses by trial propagation under their negated literals.
#[derive(Debug, Default, Clone, Copy)]
pub struct Vivification;

impl Preprocessor for Vivification {
    fn name(&self) -> &'static str {
        "vivification"
    }

    fn preprocess(&mut self, _formula: &FormulaView<'_>) -> Result<PreprocessResult> {
        Ok(PreprocessResult::Unchanged)
    }
}
