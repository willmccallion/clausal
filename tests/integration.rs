//! Integration tests: run the engine end-to-end on hand-built formulas
//! whose verdicts are known, and on small combinatorial instances.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_range_loop,
    clippy::uninlined_format_args,
    clippy::missing_const_for_fn,
    clippy::stable_sort_primitive,
    reason = "test harness uses expect/unwrap and index loops for readability"
)]

use clausal::{Lit, Polarity, Solution, Solver, Var};

fn lit(v: Var, pos: bool) -> Lit {
    Lit::new(v, if pos { Polarity::Positive } else { Polarity::Negative })
}

fn assert_satisfies(model: &clausal::Model<'_>, clauses: &[Vec<Lit>]) {
    for clause in clauses {
        let ok = clause.iter().any(|&l| model.value(l));
        assert!(ok, "model fails to satisfy clause {clause:?}");
    }
}

#[test]
fn satisfiable_2cnf_has_model() {
    let mut s = Solver::new();
    let vs = s.new_vars(4).expect("vars");
    let clauses: Vec<Vec<Lit>> = vec![
        vec![lit(vs[0], true), lit(vs[1], false)],
        vec![lit(vs[1], true), lit(vs[2], true)],
        vec![lit(vs[2], false), lit(vs[3], true)],
        vec![lit(vs[0], false), lit(vs[3], false)],
    ];
    for c in &clauses {
        s.add(c.iter().copied());
    }
    let Solution::Sat(m) = s.solve().expect("solve") else {
        panic!("expected sat");
    };
    assert_satisfies(&m, &clauses);
}

#[test]
fn pigeonhole_4_into_3_is_unsat() {
    // Four pigeons, three holes. Classic UNSAT instance.
    let pigeons = 4u32;
    let holes = 3u32;
    let mut s = Solver::new();
    let mut grid: Vec<Vec<Var>> = Vec::with_capacity(pigeons as usize);
    for _ in 0..pigeons {
        let row = s.new_vars(holes as usize).expect("vars");
        grid.push(row);
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
    assert!(matches!(s.solve().expect("solve"), Solution::Unsat(_)));
}

#[test]
fn unit_and_binary_chain_propagates_end_to_end() {
    let mut s = Solver::new();
    let vs = s.new_vars(5).expect("vars");
    s.add([lit(vs[0], true)]);
    for i in 0..vs.len() - 1 {
        s.add([lit(vs[i], false), lit(vs[i + 1], true)]);
    }
    let Solution::Sat(m) = s.solve().expect("solve") else {
        panic!("expected sat");
    };
    for v in &vs {
        assert!(m.value(lit(*v, true)), "chain should force {v:?} to true");
    }
}

#[test]
fn enumerates_all_two_variable_models() {
    let mut s = Solver::new();
    let a = s.new_var().expect("a");
    let b = s.new_var().expect("b");
    s.add([lit(a, true), lit(b, true)]);
    let mut seen: Vec<(bool, bool)> = Vec::new();
    for model in s.solutions() {
        seen.push((
            matches!(model.var_value(a), Polarity::Positive),
            matches!(model.var_value(b), Polarity::Positive),
        ));
    }
    seen.sort();
    assert_eq!(seen, vec![(false, true), (true, false), (true, true)]);
}

#[test]
fn assumption_unsat_returns_nonempty_core() {
    let mut s = Solver::new();
    let a = s.new_var().expect("a");
    let b = s.new_var().expect("b");
    s.add([lit(a, true), lit(b, true)]);
    s.add([lit(a, true), lit(b, false)]);
    let result = s.solve_under([lit(a, false)]).expect("solve_under");
    match result {
        clausal::Limited::Unsat(core) => {
            assert!(!core.is_empty(), "expected a non-empty core");
            assert!(core.lits().contains(&lit(a, false)));
        }
        other => panic!("expected unsat, got {other:?}"),
    }
}

#[test]
fn assumption_sat_yields_consistent_model() {
    let mut s = Solver::new();
    let a = s.new_var().expect("a");
    let b = s.new_var().expect("b");
    s.add([lit(a, true), lit(b, true)]);
    let result = s.solve_under([lit(a, false)]).expect("solve_under");
    match result {
        clausal::Limited::Sat(model) => {
            assert!(!model.value(lit(a, true)));
            assert!(model.value(lit(b, true)));
        }
        other => panic!("expected sat, got {other:?}"),
    }
}

#[test]
fn incremental_add_turns_sat_into_unsat() {
    let mut s = Solver::new();
    let a = s.new_var().expect("a");
    s.add([lit(a, true)]);
    assert!(matches!(s.solve().expect("first"), Solution::Sat(_)));
    s.add([lit(a, false)]);
    assert!(matches!(s.solve().expect("second"), Solution::Unsat(_)));
}
