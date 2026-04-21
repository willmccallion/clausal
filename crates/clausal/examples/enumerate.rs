//! Enumerates every satisfying assignment via `Solver::solutions`.

use clausal::{Lit, Polarity, Result, Solver};

fn main() -> Result<()> {
    let mut solver = Solver::new();
    let a = solver.new_var()?;
    let b = solver.new_var()?;

    solver.add([Lit::new(a, Polarity::Positive), Lit::new(b, Polarity::Positive)]);

    let count = solver.solutions().count();
    println!("{count} models found");
    Ok(())
}
