//! Lightweight, hand-rolled solver benchmarks.
//!
//! Runs a handful of small SAT/UNSAT instances through the engine and
//! reports wall-clock time. Intentionally not hooked into the `test`
//! crate's unstable `Bencher` so it compiles on stable Rust; invoke with
//! `cargo run --release --bench solver`.

use std::time::Instant;

use clausal::{Lit, Polarity, Solution, Solver, Var};

fn lit(v: Var, positive: bool) -> Lit {
    Lit::new(v, if positive { Polarity::Positive } else { Polarity::Negative })
}

fn pigeonhole(pigeons: u32, holes: u32) -> Solver {
    let mut s = Solver::new();
    let mut grid: Vec<Vec<Var>> = Vec::with_capacity(pigeons as usize);
    for _ in 0..pigeons {
        grid.push(s.new_vars(holes as usize).expect("vars"));
    }
    for p in 0..pigeons as usize {
        let clause: Vec<Lit> = (0..holes as usize).map(|h| lit(grid[p][h], true)).collect();
        s.add(clause);
    }
    for h in 0..holes as usize {
        for p1 in 0..pigeons as usize {
            for p2 in (p1 + 1)..pigeons as usize {
                s.add([lit(grid[p1][h], false), lit(grid[p2][h], false)]);
            }
        }
    }
    s
}

fn chain(n: usize) -> Solver {
    let mut s = Solver::new();
    let vs = s.new_vars(n).expect("vars");
    s.add([lit(vs[0], true)]);
    for i in 0..vs.len() - 1 {
        s.add([lit(vs[i], false), lit(vs[i + 1], true)]);
    }
    s
}

fn time_solve(name: &str, mut solver: Solver) {
    let start = Instant::now();
    let verdict = match solver.solve() {
        Ok(Solution::Sat(_)) => "SAT",
        Ok(Solution::Unsat(_)) => "UNSAT",
        Err(_) => "ERR",
    };
    let elapsed = start.elapsed();
    println!("{name:<24} {verdict:<6} {elapsed:?}");
}

fn main() {
    println!("{:<24} {:<6} {}", "instance", "result", "time");
    time_solve("chain-64", chain(64));
    time_solve("chain-256", chain(256));
    time_solve("php-4-into-3 (unsat)", pigeonhole(4, 3));
    time_solve("php-5-into-4 (unsat)", pigeonhole(5, 4));
    time_solve("php-6-into-5 (unsat)", pigeonhole(6, 5));
}
