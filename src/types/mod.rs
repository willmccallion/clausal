//! Public value types shared across the solver.
//!
//! Every type here is a tiny, `Copy`-friendly newtype designed so illegal
//! states are either unrepresentable or caught by the type system.

mod clause;
mod level;
mod lit;
mod value;
mod var;

pub use clause::{Clause, ClauseId};
pub use level::DecisionLevel;
pub use lit::{Lit, Polarity};
pub use value::Value;
pub use var::Var;
