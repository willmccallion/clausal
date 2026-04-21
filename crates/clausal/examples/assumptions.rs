//! Solves under a set of assumption literals using `solve_under`.

use clausal::{Limited, Lit, Polarity, Result, Solver};

fn main() -> Result<()> {
    let mut solver = Solver::new();
    let x = solver.new_var()?;
    let y = solver.new_var()?;

    solver.add([Lit::new(x, Polarity::Positive), Lit::new(y, Polarity::Positive)]);

    match solver.solve_under([Lit::new(x, Polarity::Negative)])? {
        Limited::Sat(_) => println!("sat under x=false"),
        Limited::Unsat(core) => println!("unsat core size {}", core.len()),
        Limited::Unknown(reason) => println!("unknown: {reason:?}"),
    }
    Ok(())
}
