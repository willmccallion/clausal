//! DIMACS parse/write round-trip tests.
//!
//! Proptest-generated CNFs are written out and parsed back; the result must
//! preserve variable counts, clause counts, and per-clause literal order.

#![cfg(feature = "dimacs")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    reason = "proptest strategies and test harness use expect/unwrap and integer casts"
)]

use clausal::dimacs::{CnfDimacsExt, Parser, Writer};
use clausal::{Cnf, Lit};
use proptest::prelude::*;

fn lit_strategy(max_var: u32) -> impl Strategy<Value = i32> {
    (1..=max_var as i32, any::<bool>()).prop_map(|(v, neg)| if neg { -v } else { v })
}

fn clause_strategy(max_var: u32) -> impl Strategy<Value = Vec<i32>> {
    prop::collection::vec(lit_strategy(max_var), 1..=6)
}

fn cnf_strategy() -> impl Strategy<Value = Cnf> {
    (1u32..=8u32)
        .prop_flat_map(|nvars| {
            let clauses = prop::collection::vec(clause_strategy(nvars), 1..=12);
            (Just(nvars), clauses)
        })
        .prop_map(|(nvars, raw_clauses)| {
            let mut cnf = Cnf::new();
            let _ = cnf.new_vars(nvars as usize).expect("vars");
            for raw in raw_clauses {
                let lits: Vec<Lit> =
                    raw.into_iter().filter_map(Lit::from_dimacs).collect();
                if !lits.is_empty() {
                    cnf.add(lits);
                }
            }
            cnf
        })
}

proptest! {
    #[test]
    fn write_then_parse_is_identity(cnf in cnf_strategy()) {
        let text = Writer::new().write(&cnf).expect("write");
        let back = Parser::new().parse(&text).expect("parse");
        prop_assert_eq!(cnf.num_vars(), back.num_vars());
        prop_assert_eq!(cnf.num_clauses(), back.num_clauses());
        for (a, b) in cnf.clauses().zip(back.clauses()) {
            let la: Vec<i32> = a.iter().map(|l| l.to_dimacs()).collect();
            let lb: Vec<i32> = b.iter().map(|l| l.to_dimacs()).collect();
            prop_assert_eq!(la, lb);
        }
    }

    #[test]
    fn ext_trait_round_trip(cnf in cnf_strategy()) {
        let text = cnf.to_dimacs().expect("to_dimacs");
        let back = Cnf::from_dimacs(&text).expect("from_dimacs");
        prop_assert_eq!(cnf.num_vars(), back.num_vars());
        prop_assert_eq!(cnf.num_clauses(), back.num_clauses());
    }
}

#[test]
fn tolerates_percent_eof() {
    let src = "p cnf 3 2\n1 2 0\n-1 3 0\n%\n0\n";
    let cnf = Parser::new().parse(src).expect("parse");
    assert_eq!(cnf.num_vars(), 3);
    assert_eq!(cnf.num_clauses(), 2);
}

#[test]
fn grows_variable_count_from_clause_literals() {
    let src = "p cnf 1 1\n1 7 0\n";
    let cnf = Parser::new().parse(src).expect("parse");
    assert_eq!(cnf.num_vars(), 7);
}

#[test]
fn emits_header_matching_contents() {
    let mut cnf = Cnf::new();
    let vs = cnf.new_vars(4).expect("vars");
    cnf.add([vs[0].pos(), vs[1].pos(), vs[2].neg()]);
    cnf.add([vs[3].neg(), vs[0].neg()]);
    let text = Writer::new().write(&cnf).expect("write");
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("p cnf 4 2"));
}
