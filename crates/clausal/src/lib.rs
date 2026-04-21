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

pub use clausal_core::{Clause, ClauseId, DecisionLevel, Lit, Polarity, Value, Var};
