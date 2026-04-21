//! Loads a DIMACS CNF from disk and hands it to the solver.

#[cfg(feature = "dimacs")]
fn main() -> clausal::Result<()> {
    use clausal::{Solution, Solver};
    use clausal_dimacs::Parser;
    use std::path::Path;

    let path = Path::new("examples/sample.cnf");
    let cnf = Parser::new().parse_file(path)?;
    let mut solver = Solver::builder().build_from(cnf)?;

    match solver.solve()? {
        Solution::Sat(_) => println!("sat"),
        Solution::Unsat(_) => println!("unsat"),
    }
    Ok(())
}

#[cfg(not(feature = "dimacs"))]
fn main() {
    eprintln!("build with --features dimacs to run this example");
}
