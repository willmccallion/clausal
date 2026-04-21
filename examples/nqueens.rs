//! Encodes the N-queens problem as CNF and asks the solver for a placement.

use clausal::{Lit, Polarity, Result, Solution, Solver, Var};

const N: usize = 8;

fn main() -> Result<()> {
    let mut solver = Solver::new();
    let mut cell: [[Option<Var>; N]; N] = [[None; N]; N];
    for row in &mut cell {
        for c in row.iter_mut() {
            *c = Some(solver.new_var()?);
        }
    }

    let at = |r: usize, c: usize, pol: Polarity| -> Result<Lit> {
        cell[r][c].map(|v| Lit::new(v, pol)).ok_or(clausal::Error::VariableLimitExceeded)
    };

    for r in 0..N {
        let row_any: Vec<Lit> = (0..N).map(|c| at(r, c, Polarity::Positive)).collect::<Result<_>>()?;
        solver.add(row_any);
        for c1 in 0..N {
            for c2 in (c1 + 1)..N {
                solver.add([at(r, c1, Polarity::Negative)?, at(r, c2, Polarity::Negative)?]);
            }
        }
    }
    for c in 0..N {
        for r1 in 0..N {
            for r2 in (r1 + 1)..N {
                solver.add([at(r1, c, Polarity::Negative)?, at(r2, c, Polarity::Negative)?]);
            }
        }
    }

    match solver.solve()? {
        Solution::Sat(_) => println!("found a placement"),
        Solution::Unsat(_) => println!("no placement exists"),
    }
    Ok(())
}
