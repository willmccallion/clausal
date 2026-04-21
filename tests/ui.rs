//! Drives trybuild over the compile-fail cases in `tests/ui/`.

#[test]
fn compile_failures() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/model_outlives_solver.rs");
    t.compile_fail("tests/ui/mutate_solver_while_borrowed.rs");
    t.compile_fail("tests/ui/var_zero_rejected.rs");
    #[cfg(feature = "proofs")]
    t.compile_fail("tests/ui/sealed_proof_writer.rs");
}
