//! Search-engine building blocks.
//!
//! Each submodule implements one phase of the CDCL loop as a free function
//! operating on the engine's primary mutable state (arena, assignment,
//! watchers). The pieces are composed by `search_loop` once all phases are
//! in place.

pub(crate) mod propagate;
