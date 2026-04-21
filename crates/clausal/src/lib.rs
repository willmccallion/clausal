//! A pure-Rust CDCL SAT solver.
//!
//! Re-exports the public surface of [`clausal_core`] and optionally pulls
//! in DIMACS parsing and proof emission behind Cargo features.

pub use clausal_core::builder::SolverBuilder;
pub use clausal_core::cnf::Cnf;
pub use clausal_core::context::{ClauseRef, FormulaView, SearchContext};
pub use clausal_core::error::{Error, Result};
#[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))]
pub use clausal_core::interrupter::Interrupter;
pub use clausal_core::result::{InterruptReason, Limited, Model, OwnedModel, Solution, Solutions, UnsatCore};
pub use clausal_core::solver::Solver;
pub use clausal_core::stats::Statistics;
pub use clausal_core::types::{Clause, ClauseId, DecisionLevel, Lit, Polarity, Value, Var};
