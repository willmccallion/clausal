//! Configures a conflict budget and wall-clock timeout on the builder.

use clausal::{Lit, Polarity, Result, Solution, Solver};

fn main() -> Result<()> {
    let mut solver = Solver::builder()
        .with_conflict_budget(10_000)
        .with_timeout_ms(500)
        .build();

    let a = solver.new_var()?;
    let b = solver.new_var()?;
    solver.add([Lit::new(a, Polarity::Positive), Lit::new(b, Polarity::Negative)]);

    match solver.solve()? {
        Solution::Sat(_) => println!("sat within budget"),
        Solution::Unsat(_) => println!("unsat within budget"),
    }
    Ok(())
}
