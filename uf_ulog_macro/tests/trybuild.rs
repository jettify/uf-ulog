#[test]
fn derive_ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_basic.rs");
    t.pass("tests/ui/pass_array.rs");
    t.pass("tests/ui/pass_registry.rs");
    t.compile_fail("tests/ui/fail_missing_timestamp.rs");
    t.compile_fail("tests/ui/fail_bad_timestamp_type.rs");
    t.compile_fail("tests/ui/fail_unsupported_type.rs");
    t.compile_fail("tests/ui/fail_unsupported_char.rs");
    t.compile_fail("tests/ui/fail_bad_name_attr.rs");
    t.compile_fail("tests/ui/fail_unknown_ulog_attr_key.rs");
    t.compile_fail("tests/ui/fail_generics.rs");
    t.compile_fail("tests/ui/fail_registry_non_enum.rs");
    t.compile_fail("tests/ui/fail_registry_non_unit_variant.rs");
    t.compile_fail("tests/ui/fail_registry_generics.rs");
}
