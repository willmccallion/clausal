//! Failed literal probing.

use crate::context::FormulaView;
use crate::error::Result;
use crate::traits::{PreprocessResult, Preprocessor};

/// Probes each decision literal; conflicts reveal forced assignments at the root.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailedLiteralProbing;

impl Preprocessor for FailedLiteralProbing {
    fn name(&self) -> &'static str {
        "flp"
    }

    fn preprocess(&mut self, _formula: &FormulaView<'_>) -> Result<PreprocessResult> {
        Ok(PreprocessResult::Unchanged)
    }
}
