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
