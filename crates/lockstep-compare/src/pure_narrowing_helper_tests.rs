use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, opts_report_all, pure_narrowing_helper_opts,
};

const HELPER_DECL: &str = "function asString(value) { return typeof value === \"string\" ? value : undefined; }";

#[test]
fn call_site_with_default_absorbs() {
    let base = "function f(obj) { return obj.foo; }";
    let head = format!(
        "{HELPER_DECL}
        function f(obj) {{ return asString(obj.foo) ?? \"\"; }}"
    );
    assert_equiv_raw(
        base,
        &head,
        &pure_narrowing_helper_opts(vec!["asString".into()]),
    );
}

#[test]
fn helper_declaration_filtered_when_base_has_only_consumer() {
    let base = "function f(obj) { return obj.foo; }";
    let head = format!(
        "{HELPER_DECL}
        function f(obj) {{ return asString(obj.foo) ?? \"\"; }}"
    );
    assert_equiv_raw(
        base,
        &head,
        &pure_narrowing_helper_opts(vec!["asString".into()]),
    );
}

#[test]
fn rejects_helper_not_in_config() {
    let base = "function f(obj) { return obj.foo; }";
    let head = format!(
        "{HELPER_DECL}
        function f(obj) {{ return asString(obj.foo) ?? \"\"; }}"
    );
    assert_flagged_raw(
        base,
        &head,
        &pure_narrowing_helper_opts(vec!["asNumber".into()]),
    );
}

#[test]
fn rejects_helper_in_config_but_not_declared() {
    let base = "function f(obj) { return obj.foo; }";
    let head = "function f(obj) { return asString(obj.foo) ?? \"\"; }";
    assert_flagged_raw(
        base,
        head,
        &pure_narrowing_helper_opts(vec!["asString".into()]),
    );
}

#[test]
fn rejects_call_without_nullish_default() {
    let base = "function f(obj) { return obj.foo; }";
    let head = format!(
        "{HELPER_DECL}
        function f(obj) {{ return asString(obj.foo); }}"
    );
    assert_flagged_raw(
        base,
        &head,
        &pure_narrowing_helper_opts(vec!["asString".into()]),
    );
}

#[test]
fn rejects_when_inner_expr_diverges() {
    let base = "function f(obj) { return obj.foo; }";
    let head = format!(
        "{HELPER_DECL}
        function f(obj) {{ return asString(obj.bar) ?? \"\"; }}"
    );
    assert_flagged_raw(
        base,
        &head,
        &pure_narrowing_helper_opts(vec!["asString".into()]),
    );
}

#[test]
fn composes_with_member_access() {
    let base = "class C { read() { return this.value; } }";
    let head = format!(
        "{HELPER_DECL}
        class C {{ read() {{ return asString(this.value) ?? \"\"; }} }}"
    );
    assert_equiv_raw(
        base,
        &head,
        &pure_narrowing_helper_opts(vec!["asString".into()]),
    );
}

#[test]
fn is_gated_off_by_default() {
    let base = "function f(obj) { return obj.foo; }";
    let head = format!(
        "{HELPER_DECL}
        function f(obj) {{ return asString(obj.foo) ?? \"\"; }}"
    );
    assert_flagged_raw(base, &head, &opts_report_all());
}
