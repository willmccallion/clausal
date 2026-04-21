//! Pluggable extension traits for the solver.
//!
//! Each trait is object-safe so [`SolverBuilder`] can store implementations
//! behind `Box<dyn Trait>`. Enforced at compile time by
//! [`static_assertions::assert_obj_safe!`].
//!
//! [`SolverBuilder`]: crate::builder::SolverBuilder

pub mod decision;
pub mod deletion;
pub mod preprocessor;
pub mod restart;

pub use decision::DecisionHeuristic;
pub use deletion::ClauseDeletion;
pub use preprocessor::{PreprocessResult, Preprocessor};
pub use restart::RestartStrategy;

use static_assertions::assert_obj_safe;

assert_obj_safe!(DecisionHeuristic, RestartStrategy, ClauseDeletion, Preprocessor);
