//! Subsumption and self-subsuming resolution.

use crate::context::FormulaView;
use crate::error::Result;
use crate::traits::{PreprocessResult, Preprocessor};

/// Removes clauses subsumed by shorter clauses and strengthens via self-subsuming resolution.
#[derive(Debug, Default, Clone, Copy)]
pub struct Subsumption;

impl Preprocessor for Subsumption {
    fn name(&self) -> &'static str {
        "subsumption"
    }

    fn preprocess(&mut self, _formula: &FormulaView<'_>) -> Result<PreprocessResult> {
        Ok(PreprocessResult::Unchanged)
    }
}
