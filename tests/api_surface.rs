//! Touches every publicly constructible entry point once so any signature
//! change surfaces as a compile error here.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_pass_by_value,
    clippy::bool_assert_comparison,
    reason = "surface smoke test uses expect on infallible constructors"
)]

use clausal::{
    Cnf, DecisionLevel, Error, InterruptReason, Interrupter, Limited, Lit, Model, OwnedModel,
    Polarity, Result, Solution, Solver, SolverBuilder, Statistics, UnsatCore, Value, Var,
};

fn touch_types() {
    assert!(Var::new(0).is_none());
    assert!(Lit::from_dimacs(0).is_none());
    let v = Var::new(1).expect("non-zero");
    let l = Lit::new(v, Polarity::Positive);
    assert_eq!(l.polarity(), Polarity::Positive);
    assert_eq!(l.var(), v);
    assert_eq!(!Polarity::Positive, Polarity::Negative);
    assert!(!Value::Unassigned.is_assigned());
    assert_eq!(DecisionLevel::GROUND.get(), 0);
    assert_eq!((!l).polarity(), Polarity::Negative);
}

fn touch_cnf() -> Result<Cnf> {
    let mut cnf = Cnf::new();
    let x = cnf.new_var()?;
    let y = cnf.new_var()?;
    cnf.add([Lit::new(x, Polarity::Positive), Lit::new(y, Polarity::Negative)]);
    assert_eq!(cnf.num_vars(), 2);
    assert_eq!(cnf.num_clauses(), 1);
    Ok(cnf)
}

fn touch_builder(cnf: Cnf) -> Result<Solver> {
    let builder: SolverBuilder = Solver::builder()
        .with_conflict_budget(1024)
        .with_propagation_budget(1 << 20)
        .with_timeout_ms(250)
        .with_chrono_gap(100)
        .verbose(false);
    builder.build_from(&cnf)
}

fn touch_solver(solver: &mut Solver) -> Result<()> {
    let v = solver.new_var()?;
    let _ = solver.new_vars(2)?;
    solver.add([Lit::new(v, Polarity::Positive)]);
    let _: u32 = solver.num_vars();
    let _: usize = solver.num_clauses();
    let _: Statistics = solver.statistics();
    let _: DecisionLevel = solver.decision_level();
    let _: Value = solver.value(Lit::new(v, Polarity::Negative));
    let _: Option<Model<'_>> = solver.model();
    let _: Option<UnsatCore<'_>> = solver.unsat_core();
    let _ = solver.solutions().count();

    match solver.solve() {
        Ok(Solution::Sat(_) | Solution::Unsat(_)) | Err(Error::NotImplemented) => {}
        Err(e) => return Err(e),
    }
    match solver.solve_under([Lit::new(v, Polarity::Positive)]) {
        Ok(Limited::Sat(_) | Limited::Unsat(_) | Limited::Unknown(_))
        | Err(Error::NotImplemented) => {}
        Err(e) => return Err(e),
    }
    Ok(())
}

fn touch_interrupter() {
    let a = Interrupter::new();
    let b = a.clone();
    assert!(!a.is_interrupted());
    b.interrupt();
    assert!(a.is_interrupted());
}

fn touch_owned_model() {
    let m = OwnedModel::new();
    assert!(m.is_empty());
    assert_eq!(m.len(), 0);
    let _ = m.iter().count();
}

fn touch_interrupt_reasons() {
    for r in [
        InterruptReason::Timeout,
        InterruptReason::ConflictLimit,
        InterruptReason::External,
        InterruptReason::MemoryLimit,
    ] {
        assert_eq!(format!("{r:?}").is_empty(), false);
    }
}

#[test]
fn surface_compiles_and_runs() -> Result<()> {
    touch_types();
    touch_owned_model();
    touch_interrupter();
    touch_interrupt_reasons();
    let cnf = touch_cnf()?;
    let mut solver = touch_builder(cnf)?;
    touch_solver(&mut solver)?;
    Ok(())
}
