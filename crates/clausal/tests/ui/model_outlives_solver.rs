//! A Model borrows its solver; returning it past the solver's scope must fail.

use clausal::{Model, Solver};

fn stray<'a>() -> Option<Model<'a>> {
    let s = Solver::new();
    s.model()
}

fn main() {
    let _ = stray();
}
