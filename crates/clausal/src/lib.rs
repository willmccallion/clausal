//! A pure-Rust CDCL SAT solver.
//!
//! `clausal` is an ergonomic, extensible SAT solver written in pure Rust,
//! with no C/C++ dependencies. The core is `no_std`-compatible and builds
//! for WebAssembly.
//!
//! # Status
//!
//! Early scaffolding. The public API compiles and is honest about what it
//! does — every call to [`Cnf::solve`] currently returns
//! [`Error::NotImplemented`]. The engine lands in subsequent releases.
//!
//! [`Cnf::solve`]: clausal_core::Cnf::solve
//! [`Error::NotImplemented`]: clausal_core::Error::NotImplemented

// Re-exports land alongside the first public types in the next commit.

