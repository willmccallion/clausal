//! A pure-Rust CDCL SAT solver.
//!
//! `clausal` is an ergonomic, extensible SAT solver written in pure Rust,
//! with no C/C++ dependencies. The core is `no_std`-compatible and builds
//! for WebAssembly.
//!
//! # Status
//!
//! Early scaffolding. The public API compiles and is honest about what it
//! does: every call to `solve` currently returns
//! [`Error::NotImplemented`]. The engine lands in subsequent releases.
//!
//! [`Error::NotImplemented`]: clausal_core::types

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
