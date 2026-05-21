use crate::compare_options::CompareOptions;
use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, opts_report_all, pure_narrowing_helper_opts,
    pure_narrowing_helper_strict_opts,
};

const HELPER_DECL: &str =
    "function asString(value) { return typeof value === \"string\" ? value : undefined; }";

fn with_helper(body: &str) -> String {
    format!("{HELPER_DECL}\n{body}")
}

fn opts() -> CompareOptions {
    pure_narrowing_helper_opts(vec!["asString".to_string()])
}

fn opts_strict() -> CompareOptions {
    pure_narrowing_helper_strict_opts(vec!["asString".to_string()])
}

fn assert_equiv_with(base: &str, head_body: &str) {
    let head = with_helper(head_body);
    assert_equiv_raw(base, &head, &opts());
}

fn assert_flagged_with(base: &str, head_body: &str) {
    let head = with_helper(head_body);
    assert_flagged_raw(base, &head, &opts());
}

const BASE_NAME_TEMPLATE: &str = "function f(obj) { return `name: ${obj.foo}`; }";
const BASE_BARE_FOO: &str = "function f(obj) { return obj.foo; }";

fn head_one_default(default: &str) -> String {
    format!(
        "function f(obj) {{
            const name = asString(obj.foo) ?? {default};
            return name;
        }}"
    )
}

#[test]
fn extracted_local_with_empty_string_default_absorbs() {
    assert_equiv_with(
        BASE_NAME_TEMPLATE,
        "function f(obj) {
            const name = asString(obj.foo) ?? \"\";
            return `name: ${name}`;
        }",
    );
}

#[test]
fn extracted_local_with_null_default_absorbs() {
    assert_equiv_with(BASE_BARE_FOO, &head_one_default("null"));
}

#[test]
fn extracted_local_with_undefined_default_absorbs() {
    assert_equiv_with(BASE_BARE_FOO, &head_one_default("undefined"));
}

#[test]
fn extracted_local_with_zero_default_absorbs() {
    assert_equiv_with(BASE_BARE_FOO, &head_one_default("0"));
}

#[test]
fn extracted_local_with_empty_array_default_absorbs() {
    assert_equiv_with(BASE_BARE_FOO, &head_one_default("[]"));
}

#[test]
fn type_predicate_ternary_form_absorbs() {
    let base = "function f(obj) { return `kind: ${obj.foo}`; }";
    let head = "function isPlainObject(v) { return v !== null && typeof v === \"object\"; }
        function f(obj) {
            const name = isPlainObject(obj.foo) ? obj.foo : {};
            return `kind: ${name}`;
        }";
    let opts = pure_narrowing_helper_opts(vec!["isPlainObject".to_string()]);
    assert_equiv_raw(base, head, &opts);
}

#[test]
fn read_in_multiple_positions_absorbs() {
    let base = "function f(obj) {
            log(obj.foo);
            return `name: ${obj.foo} = ${obj.foo}`;
        }";
    assert_equiv_with(
        base,
        "function f(obj) {
            const name = asString(obj.foo) ?? \"\";
            log(name);
            return `name: ${name} = ${name}`;
        }",
    );
}

#[test]
fn composes_with_inline_helper_call_in_same_block() {
    let base = "function f(obj) {
            log(obj.foo);
            return obj.bar;
        }";
    assert_equiv_with(
        base,
        "function f(obj) {
            const name = asString(obj.foo) ?? \"\";
            log(name);
            return asString(obj.bar) ?? \"\";
        }",
    );
}

#[test]
fn member_access_expr_absorbs() {
    let base = "class C { read() { return this.value; } }";
    assert_equiv_with(
        base,
        "class C { read() {
            const v = asString(this.value) ?? \"\";
            return v;
        } }",
    );
}

#[test]
fn optional_chain_expr_absorbs() {
    let base = "function f(obj) { return `name: ${obj?.foo}`; }";
    assert_equiv_with(
        base,
        "function f(obj) {
            const name = asString(obj?.foo) ?? \"\";
            return `name: ${name}`;
        }",
    );
}

#[test]
fn rejects_helper_not_in_allowlist() {
    let base = BASE_NAME_TEMPLATE;
    let head = with_helper(
        "function f(obj) {
            const name = asNumber(obj.foo) ?? 0;
            return `name: ${name}`;
        }",
    );
    let opts = pure_narrowing_helper_opts(vec!["asString".to_string()]);
    assert_flagged_raw(base, &head, &opts);
}

#[test]
fn rejects_unsafe_default() {
    assert_flagged_with(
        BASE_NAME_TEMPLATE,
        "function f(obj) {
            const name = asString(obj.foo) ?? other();
            return `name: ${name}`;
        }",
    );
}

#[test]
fn rejects_local_reassigned_mid_block() {
    let base = "function f(obj) {
            log(obj.foo);
            return obj.foo;
        }";
    assert_flagged_with(
        base,
        "function f(obj) {
            let name = asString(obj.foo) ?? \"\";
            log(name);
            name = \"other\";
            return name;
        }",
    );
}

#[test]
fn rule_gated_off_independent_of_pure_narrowing_helper() {
    let head = with_helper(
        "function f(obj) {
            const name = asString(obj.foo) ?? \"\";
            return `name: ${name}`;
        }",
    );
    assert_flagged_raw(BASE_NAME_TEMPLATE, &head, &opts_strict());
}

#[test]
fn rule_gated_off_by_default() {
    let head = with_helper(
        "function f(obj) {
            const name = asString(obj.foo) ?? \"\";
            return `name: ${name}`;
        }",
    );
    assert_flagged_raw(BASE_NAME_TEMPLATE, &head, &opts_report_all());
}

#[test]
fn rejects_expr_mismatch_at_use_site() {
    let base = "function f(obj) { return `name: ${obj.bar}`; }";
    assert_flagged_with(
        base,
        "function f(obj) {
            const name = asString(obj.foo) ?? \"\";
            return `name: ${name}`;
        }",
    );
}
