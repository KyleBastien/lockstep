use lockstep_core::Finding;

use crate::compare_options::CompareOptions;
use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, opts_report_all, pure_narrowing_helper_opts,
    pure_narrowing_helper_strict_opts,
};
use crate::walk::compare;

const HELPER_DECL: &str =
    "function asString(value) { return typeof value === \"string\" ? value : undefined; }";

fn with_helper(body: &str) -> String {
    format!("{HELPER_DECL}\n{body}")
}

fn opts() -> CompareOptions {
    pure_narrowing_helper_opts(vec!["asString".to_string()])
}

fn strict_opts() -> CompareOptions {
    pure_narrowing_helper_strict_opts(vec!["asString".to_string()])
}

fn run(base: &str, head: &str, opts: &CompareOptions) -> Vec<Finding> {
    compare(base, head, opts)
}

fn assert_equiv_with(base: &str, head_body: &str) {
    let head = with_helper(head_body);
    assert_equiv_raw(base, &head, &opts());
}

fn assert_flagged_with(base: &str, head_body: &str) {
    let head = with_helper(head_body);
    assert_flagged_raw(base, &head, &opts());
}

const BASE_TWO_FIELDS: &str = "function f(src) {
        const { user, client } = src;
        return [user, client];
    }";

const BASE_FOO_BAR: &str = "function f(src) {
        const { foo, bar } = src;
        return [foo, bar];
    }";

#[test]
fn two_field_destructure_then_narrow_absorbs() {
    assert_equiv_with(
        BASE_TWO_FIELDS,
        "function f(src) {
            const { user: userRaw, client: clientRaw } = src;
            const user = asString(userRaw) ?? \"\";
            const client = asString(clientRaw) ?? \"\";
            return [user, client];
        }",
    );
}

#[test]
fn four_field_destructure_then_narrow_absorbs() {
    let base = "function f(src) {
        const { a, b, c, d } = src;
        return [a, b, c, d];
    }";
    assert_equiv_with(
        base,
        "function f(src) {
            const { a: aRaw, b: bRaw, c: cRaw, d: dRaw } = src;
            const a = asString(aRaw) ?? \"\";
            const b = asString(bRaw) ?? \"\";
            const c = asString(cRaw) ?? \"\";
            const d = asString(dRaw) ?? \"\";
            return [a, b, c, d];
        }",
    );
}

#[test]
fn destructure_source_is_member_expression() {
    let base = "function f() {
        const { foo, bar } = this.cfg;
        return [foo, bar];
    }";
    assert_equiv_with(
        base,
        "function f() {
            const { foo: fooRaw, bar: barRaw } = this.cfg;
            const foo = asString(fooRaw) ?? \"\";
            const bar = asString(barRaw) ?? \"\";
            return [foo, bar];
        }",
    );
}

#[test]
fn composes_with_helper_call_site_downstream() {
    let base = "function f(src) {
        const { foo, bar } = src;
        return `${foo}-${bar}-${src.baz}`;
    }";
    assert_equiv_with(
        base,
        "function f(src) {
            const { foo: fooRaw, bar: barRaw } = src;
            const foo = asString(fooRaw) ?? \"\";
            const bar = asString(barRaw) ?? \"\";
            const baz = asString(src.baz) ?? \"\";
            return `${foo}-${bar}-${baz}`;
        }",
    );
}

#[test]
fn rejects_base_destructure_with_extra_field() {
    let base = "function f(src) {
        const { foo, bar, qux } = src;
        return [foo, bar, qux];
    }";
    assert_flagged_with(
        base,
        "function f(src) {
            const { foo: fooRaw, bar: barRaw } = src;
            const foo = asString(fooRaw) ?? \"\";
            const bar = asString(barRaw) ?? \"\";
            return [foo, bar];
        }",
    );
}

#[test]
fn rejects_narrow_with_unallowlisted_helper() {
    let head =
        "function asString(value) { return typeof value === \"string\" ? value : undefined; }
        function f(src) {
            const { foo: fooRaw, bar: barRaw } = src;
            const foo = asString(fooRaw) ?? \"\";
            const bar = asNumber(barRaw) ?? 0;
            return [foo, bar];
        }";
    assert_flagged_raw(BASE_FOO_BAR, head, &opts());
}

#[test]
fn rejects_when_keys_differ() {
    assert_flagged_with(
        BASE_FOO_BAR,
        "function f(src) {
            const { foo: fooRaw, baz: bazRaw } = src;
            const foo = asString(fooRaw) ?? \"\";
            const baz = asString(bazRaw) ?? \"\";
            return [foo, baz];
        }",
    );
}

#[test]
fn rejects_when_narrow_local_differs_from_key() {
    assert_flagged_with(
        BASE_FOO_BAR,
        "function f(src) {
            const { foo: fooRaw, bar: barRaw } = src;
            const foo = asString(fooRaw) ?? \"\";
            const renamed = asString(barRaw) ?? \"\";
            return [foo, renamed];
        }",
    );
}

#[test]
fn rejects_when_destructure_source_differs() {
    let base = "function f(a, b) {
        const { foo, bar } = a;
        return [foo, bar];
    }";
    assert_flagged_with(
        base,
        "function f(a, b) {
            const { foo: fooRaw, bar: barRaw } = b;
            const foo = asString(fooRaw) ?? \"\";
            const bar = asString(barRaw) ?? \"\";
            return [foo, bar];
        }",
    );
}

#[test]
fn rule_gated_off_independent_of_pure_narrowing_helper() {
    let head = with_helper(
        "function f(src) {
            const { foo: fooRaw, bar: barRaw } = src;
            const foo = asString(fooRaw) ?? \"\";
            const bar = asString(barRaw) ?? \"\";
            return [foo, bar];
        }",
    );
    assert_flagged_raw(BASE_FOO_BAR, &head, &strict_opts());
}

#[test]
fn rule_gated_off_by_default() {
    let head = with_helper(
        "function f(src) {
            const { foo: fooRaw, bar: barRaw } = src;
            const foo = asString(fooRaw) ?? \"\";
            const bar = asString(barRaw) ?? \"\";
            return [foo, bar];
        }",
    );
    let findings = run(BASE_FOO_BAR, &head, &opts_report_all());
    assert!(!findings.is_empty(), "expected divergence");
}

#[test]
fn standalone_destructure_does_not_false_fire() {
    let base = "function f(src) {
        const { foo } = src;
        return foo;
    }";
    assert_equiv_with(
        base,
        "function f(src) {
            const { foo } = src;
            return foo;
        }",
    );
}
