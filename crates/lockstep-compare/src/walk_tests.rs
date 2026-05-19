use std::path::PathBuf;

use lockstep_core::Category;

use crate::walk::{compare, CompareOptions};

fn opts() -> CompareOptions {
    CompareOptions {
        path: PathBuf::from("test.ts"),
        report_all: false,
        allow_constructor_assigned_method_equivalence: true,
        allow_closure_cache_field_alias: false,
    }
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
    let opts_all = CompareOptions {
        path: PathBuf::from("x.ts"),
        report_all: true,
        allow_constructor_assigned_method_equivalence: true,
        allow_closure_cache_field_alias: false,
    };
    let f = compare("let x = 1; let y = 2;", "let a = 1; let b = 2;", &opts_all);
    assert_eq!(f.len(), 2);
}

#[test]
fn report_all_breaks_root_arity_into_granular_findings() {
    let opts_all = CompareOptions {
        path: PathBuf::from("x.ts"),
        report_all: true,
        allow_constructor_assigned_method_equivalence: true,
        allow_closure_cache_field_alias: false,
    };
    let f = compare(
        "let a = 1; let c = 3;",
        "let a = 1; let b = 2; let c = 4;",
        &opts_all,
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

    let opts_alias = CompareOptions {
        path: PathBuf::from("x.ts"),
        report_all: true,
        allow_constructor_assigned_method_equivalence: true,
        allow_closure_cache_field_alias: true,
    };
    let f = compare(base, head, &opts_alias);
    assert!(f.is_empty(), "got: {:?}", f);
}
