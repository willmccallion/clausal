//! Crate-private engine scaffolding.
//!
//! These modules reserve the shape of the eventual CDCL engine so later
//! sessions can land propagation, analysis, and backtracking without
//! restructuring the crate. Nothing here is part of the public API.

pub(crate) mod arena;
pub(crate) mod conflict;
pub(crate) mod reason;
pub(crate) mod trail;
pub(crate) mod watcher;
