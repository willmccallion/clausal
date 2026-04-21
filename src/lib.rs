//! A pure-Rust CDCL SAT solver.
//!
//! `no_std`-compatible; depends only on `core` and `alloc`. DIMACS parsing
//! and proof emission are gated behind Cargo features.
#![no_std]

#[cfg(any(feature = "std", test))]
extern crate std;

extern crate alloc;

pub mod builder;
pub mod cnf;
pub mod context;
pub mod deletion;
pub mod error;
pub mod heuristics;
pub(crate) mod internal;
#[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))]
pub mod interrupter;
pub mod preprocessors;
pub mod restarts;
pub mod result;
pub mod solver;
pub mod stats;
pub mod traits;
pub mod types;

#[cfg(feature = "dimacs")]
pub mod dimacs;
#[cfg(feature = "proofs")]
pub mod proofs;

pub use builder::SolverBuilder;
pub use cnf::Cnf;
pub use context::{ClauseRef, FormulaView, SearchContext};
pub use error::{Error, Result};
#[cfg(all(target_has_atomic = "8", target_has_atomic = "ptr"))]
pub use interrupter::Interrupter;
pub use result::{InterruptReason, Limited, Model, OwnedModel, Solution, Solutions, UnsatCore};
pub use solver::Solver;
pub use stats::Statistics;
pub use types::{Clause, ClauseId, DecisionLevel, Lit, Polarity, Value, Var};

use static_assertions::{assert_eq_size, assert_impl_all};

assert_eq_size!(Var, Option<Var>);
assert_eq_size!(Lit, Option<Lit>);
assert_eq_size!(ClauseId, Option<ClauseId>);
assert_impl_all!(Var: Copy, Send, Sync, Eq, core::hash::Hash, Ord);
assert_impl_all!(Lit: Copy, Send, Sync, Eq, core::hash::Hash, Ord);
assert_impl_all!(ClauseId: Copy, Send, Sync, Eq, core::hash::Hash, Ord);
assert_impl_all!(Polarity: Copy, Send, Sync, Eq, core::hash::Hash, Ord);
assert_impl_all!(Value: Copy, Send, Sync, Eq, core::hash::Hash);
assert_impl_all!(DecisionLevel: Copy, Send, Sync, Eq, core::hash::Hash, Ord);
