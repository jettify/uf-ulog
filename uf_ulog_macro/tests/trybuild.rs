#[test]
fn derive_ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_basic.rs");
    t.pass("tests/ui/pass_array.rs");
}
