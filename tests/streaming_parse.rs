//! Streaming DIMACS build vs. Cnf-intermediate build equivalence.
//!
//! `SolverBuilder::build_from_reader` installs clauses straight into the
//! solver arena one at a time, while `SolverBuilder::build_from(&Cnf)`
//! goes through an intermediate [`Cnf`]. Both paths must produce solvers
//! that agree on verdict and on variable/clause counts for every input.

#![cfg(feature = "dimacs")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::format_push_string,
    clippy::uninlined_format_args,
    reason = "test harness uses expect/unwrap and format! for readability"
)]

use clausal::dimacs::Parser;
use clausal::{Solution, Solver};

fn verdict_via_cnf(src: &str) -> (u32, usize, bool) {
    let cnf = Parser::new().parse(src).expect("parse");
    let mut solver = Solver::builder().build_from(&cnf).expect("build_from");
    let sat = matches!(solver.solve().expect("solve"), Solution::Sat(_));
    (solver.num_vars(), solver.num_clauses(), sat)
}

fn verdict_via_reader(src: &str) -> (u32, usize, bool) {
    let mut solver = Solver::builder()
        .build_from_reader(src.as_bytes())
        .expect("build_from_reader");
    let sat = matches!(solver.solve().expect("solve"), Solution::Sat(_));
    (solver.num_vars(), solver.num_clauses(), sat)
}

#[test]
fn matches_on_trivial_sat() {
    let src = "p cnf 3 2\n1 -2 0\n-1 3 0\n";
    assert_eq!(verdict_via_cnf(src), verdict_via_reader(src));
}

#[test]
fn matches_on_trivial_unsat() {
    let src = "p cnf 1 2\n1 0\n-1 0\n";
    assert_eq!(verdict_via_cnf(src), verdict_via_reader(src));
}

#[test]
fn matches_on_pigeonhole_3_into_2() {
    let mut src = String::from("p cnf 6 9\n");
    for p in 0..3 {
        src.push_str(&format!("{} {} 0\n", p * 2 + 1, p * 2 + 2));
    }
    for h in 0..2 {
        for p1 in 0..3 {
            for p2 in (p1 + 1)..3 {
                src.push_str(&format!("-{} -{} 0\n", p1 * 2 + h + 1, p2 * 2 + h + 1));
            }
        }
    }
    assert_eq!(verdict_via_cnf(&src), verdict_via_reader(&src));
}

#[test]
fn matches_when_clause_references_unheader_vars() {
    let src = "p cnf 1 1\n1 5 0\n";
    assert_eq!(verdict_via_cnf(src), verdict_via_reader(src));
}

#[test]
fn matches_across_comments_and_percent_eof() {
    let src = "c comment\nc another\np cnf 3 2\n1 2 0\n-1 3 0\n%\n0\n";
    assert_eq!(verdict_via_cnf(src), verdict_via_reader(src));
}

#[test]
fn matches_with_multiple_clauses_per_line() {
    let src = "p cnf 3 3\n1 2 0 2 3 0 -1 -3 0\n";
    assert_eq!(verdict_via_cnf(src), verdict_via_reader(src));
}

#[test]
fn reader_path_rejects_missing_header() {
    let src = "1 -2 0\n";
    let res = Solver::builder().build_from_reader(src.as_bytes());
    assert!(res.is_err());
}

#[test]
fn reader_path_rejects_unterminated_clause() {
    let src = "p cnf 2 1\n1 -2\n";
    let res = Solver::builder().build_from_reader(src.as_bytes());
    assert!(res.is_err());
}
