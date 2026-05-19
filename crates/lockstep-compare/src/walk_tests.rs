use std::path::PathBuf;

use lockstep_core::Category;

use crate::walk::{compare, CompareOptions};

#[derive(Default)]
struct OptsOverrides {
    report_all: bool,
    cache_alias: bool,
    array_first_tier1: bool,
    array_first_tier2: bool,
}

fn build_opts(over: OptsOverrides) -> CompareOptions {
    CompareOptions {
        path: PathBuf::from("test.ts"),
        report_all: over.report_all,
        allow_constructor_assigned_method_equivalence: true,
        allow_closure_cache_field_alias: over.cache_alias,
        allow_array_first_element_or_null: over.array_first_tier1,
        allow_array_first_element_or_null_loose: over.array_first_tier2,
    }
}

fn opts() -> CompareOptions {
    build_opts(OptsOverrides::default())
}

fn opts_report_all() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        ..OptsOverrides::default()
    })
}

fn tier1_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        array_first_tier1: true,
        ..OptsOverrides::default()
    })
}

fn tier2_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        array_first_tier1: true,
        array_first_tier2: true,
        ..OptsOverrides::default()
    })
}

fn cache_alias_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        cache_alias: true,
        ..OptsOverrides::default()
    })
}

#[test]
fn identical_sources_have_no_findings() {
    let src = "function f(x) { return x + 1; }";
    let f = compare(src, src, &opts());
    assert!(f.is_empty(), "got: {:?}", f);
}

#[test]
fn renamed_identifier_flags_token_mismatch() {
    let f = compare("let x = 1;", "let y = 1;", &opts());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].category, Category::TokenMismatch);
}

#[test]
fn extra_statement_flags_arity_mismatch() {
    let f = compare("let x = 1;", "let x = 1; let y = 2;", &opts());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].category, Category::DroppedStatement);
}

#[test]
fn changed_node_kind_flags_kind_mismatch() {
    let f = compare("let x = 1;", "let x = foo();", &opts());
    assert_eq!(f.len(), 1);
    assert!(matches!(
        f[0].category,
        Category::KindMismatch | Category::ArityMismatch
    ));
}

#[test]
fn comments_are_ignored() {
    let f = compare("let x = 1; // a", "let x = 1; /* b */", &opts());
    assert!(f.is_empty(), "got: {:?}", f);
}

#[test]
fn quote_style_does_not_flag() {
    let f = compare("let s = 'foo';", "let s = \"foo\";", &opts());
    assert!(f.is_empty(), "got: {:?}", f);
}

#[test]
fn plus_vs_minus_operator_flags_divergence() {
    let f = compare(
        "function add(a, b) { return a + b; }",
        "function add(a, b) { return a - b; }",
        &opts(),
    );
    assert!(!f.is_empty(), "expected divergence");
    assert!(matches!(
        f[0].category,
        Category::KindMismatch | Category::TokenMismatch
    ));
}

#[test]
fn changed_literal_value_flags_divergence() {
    let f = compare("let x = 1;", "let x = 2;", &opts());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].category, Category::TokenMismatch);
}

#[test]
fn report_all_returns_multiple_findings() {
    let f = compare(
        "let x = 1; let y = 2;",
        "let a = 1; let b = 2;",
        &opts_report_all(),
    );
    assert_eq!(f.len(), 2);
}

#[test]
fn report_all_breaks_root_arity_into_granular_findings() {
    let f = compare(
        "let a = 1; let c = 3;",
        "let a = 1; let b = 2; let c = 4;",
        &opts_report_all(),
    );
    assert!(f
        .iter()
        .any(|finding| finding.category == Category::DroppedStatement));
    assert!(f
        .iter()
        .any(|finding| finding.category == Category::TokenMismatch));
    assert!(!f
        .iter()
        .any(|finding| finding.message.contains("`program` has")));
}

#[test]
fn constructor_assigned_function_matches_class_method() {
    let base =
        "class C { constructor() { this.getInvoice = function(id) { return this.fetch(id); }; } }";
    let head = "class C { constructor() {} getInvoice(id) { return this.fetch(id); } }";
    let f = compare(base, head, &opts());
    assert!(f.is_empty(), "got: {:?}", f);
}

#[test]
fn constructor_assigned_function_body_still_compares() {
    let base =
        "class C { constructor() { this.getInvoice = function(id) { return this.fetch(id); }; } }";
    let head = "class C { constructor() {} getInvoice(id) { return this.fetch(id + 1); } }";
    let f = compare(base, head, &opts());
    assert!(!f.is_empty());
}

#[test]
fn cache_alias_is_config_gated() {
    let base = "class C { constructor() { const invoiceCache = {}; this.getInvoice = function(id) { return invoiceCache[id]; }; } }";
    let head = "class C { _invoiceCache = {}; constructor() {} getInvoice(id) { return this._invoiceCache[id]; } }";
    let f = compare(base, head, &opts());
    assert!(!f.is_empty());

    let f = compare(base, head, &cache_alias_opts());
    assert!(f.is_empty(), "got: {:?}", f);
}

#[test]
fn cache_alias_matches_constructor_assigned_head_cache() {
    let base = "class C { constructor(cacheInvoice) { const invoiceCache = cacheInvoice; this.getInvoice = function(id) { return invoiceCache[id]; }; } }";
    let head = "class C { constructor(cacheInvoice) { this._invoiceCache = cacheInvoice; } getInvoice(id) { return this._invoiceCache[id]; } }";
    let f = compare(base, head, &cache_alias_opts());
    assert!(f.is_empty(), "got: {:?}", f);
}

#[test]
fn cache_alias_constructor_assigned_head_cache_is_config_gated() {
    let base = "class C { constructor(cacheInvoice) { const invoiceCache = cacheInvoice; this.getInvoice = function(id) { return invoiceCache[id]; }; } }";
    let head = "class C { constructor(cacheInvoice) { this._invoiceCache = cacheInvoice; } getInvoice(id) { return this._invoiceCache[id]; } }";
    let f = compare(base, head, &opts());
    assert!(!f.is_empty());
}

#[test]
fn cache_alias_constructor_assignment_value_must_match() {
    let base = "class C { constructor(cacheInvoice) { const invoiceCache = cacheInvoice; this.getInvoice = function(id) { return invoiceCache[id]; }; } }";
    let head = "class C { constructor(cacheInvoice) { this._invoiceCache = cacheInvoice ?? null; } getInvoice(id) { return this._invoiceCache[id]; } }";
    let f = compare(base, head, &cache_alias_opts());
    assert!(!f.is_empty(), "value mismatch should still flag");
}

fn compare_exprs(
    base_expr: &str,
    head_expr: &str,
    opts: &CompareOptions,
) -> Vec<lockstep_core::Finding> {
    let base = format!("let v = {base_expr};");
    let head = format!("let v = {head_expr};");
    compare(&base, &head, opts)
}

fn assert_equiv(base_expr: &str, head_expr: &str, opts: &CompareOptions) {
    let f = compare_exprs(base_expr, head_expr, opts);
    assert!(f.is_empty(), "expected no findings, got: {:?}", f);
}

fn assert_flagged(base_expr: &str, head_expr: &str, opts: &CompareOptions) {
    let f = compare_exprs(base_expr, head_expr, opts);
    assert!(!f.is_empty(), "expected divergence to be flagged");
}

#[test]
fn tier1_length_check_is_gated_off_by_default() {
    assert_flagged("arr.length > 0 ? arr[0] : null", "arr[0] ?? null", &opts());
}

#[test]
fn tier1_length_check_passes_with_flag_on() {
    assert_equiv(
        "arr.length > 0 ? arr[0] : null",
        "arr[0] ?? null",
        &tier1_opts(),
    );
}

#[test]
fn tier1_and_length_check_passes_with_flag_on() {
    assert_equiv(
        "arr && arr.length > 0 ? arr[0] : null",
        "arr[0] ?? null",
        &tier1_opts(),
    );
}

#[test]
fn tier1_optional_chain_length_check_passes_with_flag_on() {
    assert_equiv(
        "res?.data?.length > 0 ? res?.data[0] : null",
        "res?.data?.[0] ?? null",
        &tier1_opts(),
    );
}

#[test]
fn tier1_optional_mismatch_still_flags() {
    assert_flagged(
        "arr.length > 0 ? arr[0] : null",
        "arr?.[0] ?? null",
        &tier1_opts(),
    );
}

#[test]
fn tier1_different_expr_still_flags() {
    assert_flagged(
        "arr.length > 0 ? brr[0] : null",
        "arr[0] ?? null",
        &tier1_opts(),
    );
}

#[test]
fn tier1_non_zero_index_still_flags() {
    assert_flagged(
        "arr.length > 0 ? arr[1] : null",
        "arr[1] ?? null",
        &tier1_opts(),
    );
}

#[test]
fn tier1_does_not_accept_or_null_base() {
    assert_flagged("arr[0] || null", "arr[0] ?? null", &tier1_opts());
}

#[test]
fn tier2_or_null_passes_with_loose_flag() {
    assert_equiv("arr[0] || null", "arr[0] ?? null", &tier2_opts());
}

#[test]
fn tier2_bare_subscript_passes_with_loose_flag() {
    assert_equiv("res?.data[0]", "res?.data[0] ?? null", &tier2_opts());
}

#[test]
fn tier1_wrong_fallback_does_not_trigger() {
    assert_flagged(
        "arr.length > 0 ? arr[0] : null",
        "arr[0] ?? undefined",
        &tier1_opts(),
    );
}
