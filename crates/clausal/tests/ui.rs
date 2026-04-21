//! Drives trybuild over the compile-fail cases in `tests/ui/`.

#[test]
fn compile_failures() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
