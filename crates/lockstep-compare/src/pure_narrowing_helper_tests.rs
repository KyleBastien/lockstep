use lockstep_core::Finding;

use crate::compare_options::CompareOptions;
use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, opts_report_all, pure_narrowing_helper_opts,
};
use crate::walk::compare;

const HELPER_DECL: &str =
    "function asString(value) { return typeof value === \"string\" ? value : undefined; }";

const BASE_BARE_READ: &str = "function f(obj) { return obj.foo; }";

fn with_helper(body: &str) -> String {
    format!("{HELPER_DECL}\n        {body}")
}

fn opts_for(helpers: &[&str]) -> CompareOptions {
    pure_narrowing_helper_opts(helpers.iter().map(|s| s.to_string()).collect())
}

fn run(base: &str, head: &str, helpers: &[&str]) -> Vec<Finding> {
    compare(base, head, &opts_for(helpers))
}

#[test]
fn call_site_with_default_absorbs() {
    let head = with_helper("function f(obj) { return asString(obj.foo) ?? \"\"; }");
    assert_equiv_raw(BASE_BARE_READ, &head, &opts_for(&["asString"]));
}

#[test]
fn helper_declaration_filtered_when_base_has_only_consumer() {
    let head = with_helper("function f(obj) { return asString(obj.foo) ?? \"\"; }");
    assert_equiv_raw(BASE_BARE_READ, &head, &opts_for(&["asString"]));
}

#[test]
fn rejects_helper_not_in_config() {
    let head = with_helper("function f(obj) { return asString(obj.foo) ?? \"\"; }");
    let findings = run(BASE_BARE_READ, &head, &["asNumber"]);
    assert!(!findings.is_empty(), "expected divergence");
}

#[test]
fn rejects_helper_in_config_but_not_declared() {
    let head = "function f(obj) { return asString(obj.foo) ?? \"\"; }";
    assert_flagged_raw(BASE_BARE_READ, head, &opts_for(&["asString"]));
}

#[test]
fn rejects_call_without_nullish_default() {
    let head = with_helper("function f(obj) { return asString(obj.foo); }");
    assert_flagged_raw(BASE_BARE_READ, &head, &opts_for(&["asString"]));
}

#[test]
fn rejects_when_inner_expr_diverges() {
    let head = with_helper("function f(obj) { return asString(obj.bar) ?? \"\"; }");
    assert_flagged_raw(BASE_BARE_READ, &head, &opts_for(&["asString"]));
}

#[test]
fn composes_with_member_access() {
    let base = "class C { read() { return this.value; } }";
    let head = with_helper("class C { read() { return asString(this.value) ?? \"\"; } }");
    assert_equiv_raw(base, &head, &opts_for(&["asString"]));
}

#[test]
fn is_gated_off_by_default() {
    let head = with_helper("function f(obj) { return asString(obj.foo) ?? \"\"; }");
    assert_flagged_raw(BASE_BARE_READ, &head, &opts_report_all());
}

const PREDICATE_DECL: &str = "function isPlainObject(value) { return value !== null && typeof value === \"object\" && !Array.isArray(value); }";

fn with_predicate(body: &str) -> String {
    format!("{PREDICATE_DECL}\n        {body}")
}

fn predicate_pair(param: &str, expr: &str, default_lit: &str) -> (String, String) {
    let base = format!("function f({param}) {{ return {expr}; }}");
    let head = with_predicate(&format!(
        "function f({param}) {{ return isPlainObject({expr}) ? {expr} : {default_lit}; }}"
    ));
    (base, head)
}

fn assert_predicate_equiv(param: &str, expr: &str, default_lit: &str) {
    let (base, head) = predicate_pair(param, expr, default_lit);
    assert_equiv_raw(&base, &head, &opts_for(&["isPlainObject"]));
}

fn assert_predicate_flagged(param: &str, expr: &str, default_lit: &str) {
    let (base, head) = predicate_pair(param, expr, default_lit);
    assert_flagged_raw(&base, &head, &opts_for(&["isPlainObject"]));
}

#[test]
fn type_predicate_ternary_with_object_default_absorbs() {
    assert_predicate_equiv("updated", "updated", "{}");
}

#[test]
fn type_predicate_ternary_with_array_default_absorbs() {
    assert_predicate_equiv("rows", "rows", "[]");
}

#[test]
fn type_predicate_ternary_with_undefined_default_absorbs() {
    assert_predicate_equiv("updated", "updated", "undefined");
}

#[test]
fn type_predicate_ternary_with_member_argument_absorbs() {
    assert_predicate_equiv("obj", "obj.foo", "{}");
}

#[test]
fn type_predicate_ternary_rejects_consequence_differs_from_argument() {
    let base = "function f(updated) { return updated; }";
    let head =
        with_predicate("function f(updated) { return isPlainObject(updated) ? other : {}; }");
    assert_flagged_raw(base, &head, &opts_for(&["isPlainObject"]));
}

#[test]
fn type_predicate_ternary_rejects_non_safe_default() {
    assert_predicate_flagged("updated", "updated", "someComputation()");
}

#[test]
fn type_predicate_ternary_rejects_helper_not_in_config() {
    let (base, head) = predicate_pair("updated", "updated", "{}");
    let findings = run(&base, &head, &["asString"]);
    assert!(!findings.is_empty(), "expected divergence");
}

#[test]
fn type_predicate_ternary_is_gated_off_by_default() {
    let (base, head) = predicate_pair("updated", "updated", "{}");
    assert_flagged_raw(&base, &head, &opts_report_all());
}
