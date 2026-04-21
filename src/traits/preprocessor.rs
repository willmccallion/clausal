//! Formula preprocessing and inprocessing.

use crate::context::FormulaView;
use crate::error::Result;

/// Outcome of a preprocessing pass.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreprocessResult {
    /// The formula was not modified.
    Unchanged,
    /// The formula was simplified but remains satisfiability-equivalent.
    Simplified,
    /// Preprocessing proved the formula unsatisfiable outright.
    Unsat,
}

/// Transforms the formula before or during search.
pub trait Preprocessor: Send + 'static {
    /// A short human-readable name.
    fn name(&self) -> &'static str;

    /// Runs one pass of the preprocessor against the formula.
    fn preprocess(&mut self, formula: &FormulaView<'_>) -> Result<PreprocessResult>;
}
