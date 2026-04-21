//! Var::new returns Option; using the result directly as Var must fail.

use clausal::Var;

fn main() {
    let _: Var = Var::new(0);
}
