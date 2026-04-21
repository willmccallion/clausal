//! Installs a custom decision heuristic on the builder.

use clausal::{Lit, Polarity, Result, SearchContext, Solution, Solver, Var};
use clausal_core::traits::DecisionHeuristic;
use clausal_core::types::DecisionLevel;

#[derive(Debug, Default)]
struct AlwaysSmallest {
    next: u32,
    max: u32,
}

impl DecisionHeuristic for AlwaysSmallest {
    fn name(&self) -> &'static str {
        "always-smallest"
    }

    fn pick_branch(&mut self, _ctx: &SearchContext<'_>) -> Option<Lit> {
        while self.next < self.max {
            self.next += 1;
            if let Some(v) = Var::new(self.next) {
                return Some(Lit::new(v, Polarity::Positive));
            }
        }
        None
    }

    fn on_assign(&mut self, _lit: Lit, _level: DecisionLevel) {}
}

fn main() -> Result<()> {
    let mut solver = Solver::builder().with_decision_heuristic(AlwaysSmallest::default()).build();
    let a = solver.new_var()?;
    solver.add([Lit::new(a, Polarity::Positive)]);
    match solver.solve()? {
        Solution::Sat(_) => println!("sat"),
        Solution::Unsat(_) => println!("unsat"),
    }
    Ok(())
}
