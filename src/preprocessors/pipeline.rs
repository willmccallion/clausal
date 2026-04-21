//! Composite preprocessor that runs children in order.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::context::FormulaView;
use crate::error::Result;
use crate::traits::{PreprocessResult, Preprocessor};

/// A sequence of preprocessors invoked in order.
///
/// Stops on [`PreprocessResult::Unsat`].
#[derive(Default)]
pub struct Pipeline {
    stages: Vec<Box<dyn Preprocessor>>,
}

impl Pipeline {
    /// Creates an empty pipeline.
    #[must_use]
    pub const fn new() -> Self {
        Self { stages: Vec::new() }
    }

    /// Appends a preprocessor stage.
    pub fn push(&mut self, stage: Box<dyn Preprocessor>) {
        self.stages.push(stage);
    }

    /// Returns the number of stages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stages.len()
    }

    /// Returns `true` if the pipeline has no stages.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }
}

impl core::fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Pipeline").field("stages", &self.stages.len()).finish()
    }
}

impl Preprocessor for Pipeline {
    fn name(&self) -> &'static str {
        "pipeline"
    }

    fn preprocess(&mut self, formula: &FormulaView<'_>) -> Result<PreprocessResult> {
        let mut overall = PreprocessResult::Unchanged;
        for stage in &mut self.stages {
            match stage.preprocess(formula)? {
                PreprocessResult::Unchanged => {}
                PreprocessResult::Simplified => overall = PreprocessResult::Simplified,
                PreprocessResult::Unsat => return Ok(PreprocessResult::Unsat),
            }
        }
        Ok(overall)
    }
}
