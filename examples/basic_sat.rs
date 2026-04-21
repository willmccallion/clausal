//! Builds a tiny 3-variable formula and runs it through the solver.

use clausal::{Lit, Polarity, Result, Solution, Solver, Var};

fn main() -> Result<()> {
    let mut solver = Solver::new();
    let vars: Vec<Var> = (0..3).map(|_| solver.new_var()).collect::<Result<_>>()?;
    let lit = |i: usize, pol: Polarity| Lit::new(vars[i], pol);

    solver.add([lit(0, Polarity::Positive), lit(1, Polarity::Positive)]);
    solver.add([lit(1, Polarity::Negative), lit(2, Polarity::Positive)]);
    solver.add([lit(0, Polarity::Negative), lit(2, Polarity::Negative)]);

    match solver.solve()? {
        Solution::Sat(model) => println!("sat with {} variables", model.len()),
        Solution::Unsat(core) => println!("unsat core size {}", core.len()),
    }
    Ok(())
}
