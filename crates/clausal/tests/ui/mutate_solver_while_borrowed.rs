//! Cannot mutate the solver while a Model borrow is alive.

use clausal::Solver;

fn main() {
    let mut s = Solver::new();
    let model = s.model();
    s.add(core::iter::empty());
    drop(model);
}
