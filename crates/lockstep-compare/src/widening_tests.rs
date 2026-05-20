use lockstep_core::Category;

use crate::test_helpers::{
    assert_equiv, assert_flagged, build_opts, opts_report_all, widening_opts,
    widening_plus_cache_alias_opts, widening_plus_swap_opts, widening_plus_tier1_opts,
    OptsOverrides,
};
use crate::compare_options::CompareOptions;
use crate::walk::compare;

#[test]
fn nullish_widening_equivalences() {
    let cases: &[(&str, &str, &CompareOptions)] = &[
        ("value", "value ?? null", &widening_opts()),
        ("value", "value ?? undefined", &widening_opts()),
        ("value", "(value ?? null)", &widening_opts()),
        ("result.data[0]", "result.data[0] ?? null", &widening_opts()),
        (
            "data.length > 0 ? data[0] : null",
            "data[0] ?? null",
            &widening_plus_tier1_opts(),
        ),
        ("null", "undefined", &widening_plus_swap_opts()),
        ("undefined", "null", &widening_plus_swap_opts()),
    ];
    for (base, head, opts) in cases {
        assert_equiv(base, head, opts);
    }
}

#[test]
fn nullish_widening_flagged_cases() {
    let only_swap = build_opts(OptsOverrides {
        report_all: true,
        null_undefined_swap: true,
        ..OptsOverrides::default()
    });
    let cases: &[(&str, &str, &CompareOptions)] = &[
        ("value", "value ?? null", &opts_report_all()),
        ("value ?? null", "value", &widening_opts()),
        ("value", "value ?? defaultValue", &widening_opts()),
        ("value", "value ?? 0", &widening_opts()),
        ("value", "value ?? \"\"", &widening_opts()),
        ("value", "value || null", &widening_opts()),
        ("foo", "bar ?? null", &widening_opts()),
        ("null", "undefined", &widening_opts()),
        ("undefined", "null", &widening_opts()),
        ("null", "undefined", &only_swap),
    ];
    for (base, head, opts) in cases {
        assert_flagged(base, head, opts);
    }
}

#[test]
fn nullish_widening_composes_with_cache_alias() {
    let base = "class C { constructor(c) { const invoiceCache = c; this.fetch = function(id) { return invoiceCache[id]; }; } }";
    let head = "class C { constructor(c) { this._invoiceCache = c ?? null; } fetch(id) { return this._invoiceCache[id]; } }";
    let f = compare(base, head, &widening_plus_cache_alias_opts());
    assert!(f.is_empty(), "got: {:?}", f);
}

#[test]
fn nullish_widening_scratch_context_isolation() {
    let f = compare(
        "let a = x; let b = y;",
        "let a = x ?? null; let b = q;",
        &widening_opts(),
    );
    assert_eq!(
        f.len(),
        1,
        "expected only the real divergence, got: {:?}",
        f
    );
    assert_eq!(f[0].category, Category::TokenMismatch);
}
