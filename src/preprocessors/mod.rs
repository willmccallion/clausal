//! Built-in preprocessors and the composite pipeline.

pub mod bounded_variable_elimination;
pub mod failed_literal_probing;
pub mod pipeline;
pub mod pure_literal_elimination;
pub mod subsumption;
pub mod unit_propagation;
pub mod vivification;

pub use bounded_variable_elimination::BoundedVariableElimination;
pub use failed_literal_probing::FailedLiteralProbing;
pub use pipeline::Pipeline;
pub use pure_literal_elimination::PureLiteralElimination;
pub use subsumption::Subsumption;
pub use unit_propagation::UnitPropagation;
pub use vivification::Vivification;
