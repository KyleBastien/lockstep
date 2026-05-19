use std::path::PathBuf;

use lockstep_core::Finding;

use crate::walk::{compare, CompareOptions};

#[derive(Default)]
pub(crate) struct OptsOverrides {
    pub(crate) report_all: bool,
    pub(crate) cache_alias: bool,
    pub(crate) array_first_tier1: bool,
    pub(crate) array_first_tier2: bool,
    pub(crate) nullish_widening: bool,
    pub(crate) null_undefined_swap: bool,
}

pub(crate) fn build_opts(over: OptsOverrides) -> CompareOptions {
    CompareOptions {
        path: PathBuf::from("test.ts"),
        report_all: over.report_all,
        allow_constructor_assigned_method_equivalence: true,
        allow_closure_cache_field_alias: over.cache_alias,
        allow_array_first_element_or_null: over.array_first_tier1,
        allow_array_first_element_or_null_loose: over.array_first_tier2,
        allow_nullish_widening: over.nullish_widening,
        allow_null_undefined_swap: over.null_undefined_swap,
    }
}

pub(crate) fn opts() -> CompareOptions {
    build_opts(OptsOverrides::default())
}

pub(crate) fn opts_report_all() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        ..OptsOverrides::default()
    })
}

pub(crate) fn tier1_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        array_first_tier1: true,
        ..OptsOverrides::default()
    })
}

pub(crate) fn tier2_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        array_first_tier1: true,
        array_first_tier2: true,
        ..OptsOverrides::default()
    })
}

pub(crate) fn cache_alias_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        cache_alias: true,
        ..OptsOverrides::default()
    })
}

pub(crate) fn cache_alias_plus_tier1_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        cache_alias: true,
        array_first_tier1: true,
        ..OptsOverrides::default()
    })
}

pub(crate) fn widening_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        nullish_widening: true,
        ..OptsOverrides::default()
    })
}

pub(crate) fn widening_plus_swap_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        nullish_widening: true,
        null_undefined_swap: true,
        ..OptsOverrides::default()
    })
}

pub(crate) fn widening_plus_tier1_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        nullish_widening: true,
        array_first_tier1: true,
        ..OptsOverrides::default()
    })
}

pub(crate) fn widening_plus_cache_alias_opts() -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        nullish_widening: true,
        cache_alias: true,
        ..OptsOverrides::default()
    })
}

pub(crate) fn compare_exprs(
    base_expr: &str,
    head_expr: &str,
    opts: &CompareOptions,
) -> Vec<Finding> {
    let base = format!("let v = {base_expr};");
    let head = format!("let v = {head_expr};");
    compare(&base, &head, opts)
}

pub(crate) fn assert_equiv(base_expr: &str, head_expr: &str, opts: &CompareOptions) {
    let f = compare_exprs(base_expr, head_expr, opts);
    assert!(f.is_empty(), "expected no findings, got: {:?}", f);
}

pub(crate) fn assert_flagged(base_expr: &str, head_expr: &str, opts: &CompareOptions) {
    let f = compare_exprs(base_expr, head_expr, opts);
    assert!(!f.is_empty(), "expected divergence to be flagged");
}
