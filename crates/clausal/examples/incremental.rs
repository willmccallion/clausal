//! Adds clauses between solve calls to demonstrate incremental use.

use clausal::{Lit, Polarity, Result, Solution, Solver};

fn main() -> Result<()> {
    let mut solver = Solver::new();
    let a = solver.new_var()?;
    let b = solver.new_var()?;

    solver.add([Lit::new(a, Polarity::Positive), Lit::new(b, Polarity::Positive)]);
    report(solver.solve())?;

    solver.add([Lit::new(a, Polarity::Negative)]);
    report(solver.solve())?;

    Ok(())
}

fn report(outcome: Result<Solution<'_>>) -> Result<()> {
    match outcome? {
        Solution::Sat(_) => println!("sat"),
        Solution::Unsat(_) => println!("unsat"),
    }
    Ok(())
}
