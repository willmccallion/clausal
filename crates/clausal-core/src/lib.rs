//! Core types, traits, and engine scaffolding for the [clausal] SAT solver.
//!
//! This crate is `no_std`-compatible; it depends only on `core` and `alloc`.
//! The top-level [`clausal`](https://docs.rs/clausal) crate re-exports the
//! public surface and adds `std`-gated conveniences, DIMACS parsing, proof
//! emission, and an IPASIR C ABI.
//!
//! # Status
//!
//! Early scaffolding. No solver engine is implemented yet; every `solve`
//! method returns [`Error::NotImplemented`]. The public API shape is stable
//! enough to write examples against.
//!
//! [clausal]: https://crates.io/crates/clausal
#![no_std]

extern crate alloc;

pub mod cnf;
pub mod error;
pub mod types;

pub use cnf::Cnf;
pub use error::{Error, Result};
pub use types::{Clause, ClauseId, DecisionLevel, Lit, Polarity, Value, Var};

use static_assertions::{assert_eq_size, assert_impl_all};

// Compile-time guarantees: niche optimisations hold and handle types are
// thread-safe and cheap to copy.
assert_eq_size!(Var, Option<Var>);
assert_eq_size!(Lit, Option<Lit>);
assert_eq_size!(ClauseId, Option<ClauseId>);
assert_impl_all!(Var: Copy, Send, Sync, Eq, core::hash::Hash, Ord);
assert_impl_all!(Lit: Copy, Send, Sync, Eq, core::hash::Hash, Ord);
assert_impl_all!(ClauseId: Copy, Send, Sync, Eq, core::hash::Hash, Ord);
assert_impl_all!(Polarity: Copy, Send, Sync, Eq, core::hash::Hash, Ord);
assert_impl_all!(Value: Copy, Send, Sync, Eq, core::hash::Hash);
assert_impl_all!(DecisionLevel: Copy, Send, Sync, Eq, core::hash::Hash, Ord);
