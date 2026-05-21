use std::collections::HashMap;
use std::path::PathBuf;

use lockstep_core::Finding;

use crate::compare_options::CompareOptions;
use crate::walk::compare;

#[derive(Default)]
pub(crate) struct OptsOverrides {
    pub(crate) report_all: bool,
    pub(crate) cache_alias: bool,
    pub(crate) array_first_tier1: bool,
    pub(crate) array_first_tier2: bool,
    pub(crate) nullish_widening: bool,
    pub(crate) null_undefined_swap: bool,
    pub(crate) iife_async_wrapper: bool,
    pub(crate) transient_cache_wrap: bool,
    pub(crate) request_field_narrowing: bool,
    pub(crate) async_propagation: bool,
    pub(crate) defensive_null_guard: bool,
    pub(crate) non_null_alias_local: bool,
    pub(crate) defensive_log_guard: bool,
    pub(crate) defensive_log_guard_methods: Option<Vec<String>>,
    pub(crate) dead_defensive_optional_chain_removal: bool,
    pub(crate) unknown_catch_narrowing: bool,
    pub(crate) promise_settled_discrimination: bool,
    pub(crate) pure_narrowing_helper: bool,
    pub(crate) narrowing_helpers: Option<Vec<String>>,
    pub(crate) helper_call_site_substitution: bool,
    pub(crate) destructure_then_narrow: bool,
    pub(crate) narrowing_helpers_unwrap: Option<HashMap<String, String>>,
    pub(crate) narrowing_helpers_aliases: Option<HashMap<String, String>>,
}

pub(crate) fn build_opts(over: OptsOverrides) -> CompareOptions {
    let methods = over.defensive_log_guard_methods.unwrap_or_else(|| {
        ["debug", "info", "warn", "error", "trace", "log"]
            .into_iter()
            .map(String::from)
            .collect()
    });
    CompareOptions {
        path: PathBuf::from("test.ts"),
        report_all: over.report_all,
        allow_constructor_assigned_method_equivalence: true,
        allow_closure_cache_field_alias: over.cache_alias,
        allow_array_first_element_or_null: over.array_first_tier1,
        allow_array_first_element_or_null_loose: over.array_first_tier2,
        allow_nullish_widening: over.nullish_widening,
        allow_null_undefined_swap: over.null_undefined_swap,
        allow_iife_async_wrapper: over.iife_async_wrapper,
        allow_transient_cache_wrap: over.transient_cache_wrap,
        allow_request_field_narrowing: over.request_field_narrowing,
        allow_async_propagation: over.async_propagation,
        allow_defensive_null_guard: over.defensive_null_guard,
        allow_non_null_alias_local: over.non_null_alias_local,
        allow_defensive_log_guard: over.defensive_log_guard,
        defensive_log_guard_methods: methods,
        allow_dead_defensive_optional_chain_removal: over.dead_defensive_optional_chain_removal,
        allow_unknown_catch_narrowing: over.unknown_catch_narrowing,
        allow_promise_settled_discrimination: over.promise_settled_discrimination,
        allow_pure_narrowing_helper: over.pure_narrowing_helper,
        narrowing_helpers: over.narrowing_helpers.unwrap_or_default(),
        allow_helper_call_site_substitution: over.helper_call_site_substitution,
        allow_destructure_then_narrow: over.destructure_then_narrow,
        narrowing_helpers_unwrap: over.narrowing_helpers_unwrap.unwrap_or_default(),
        narrowing_helpers_aliases: over.narrowing_helpers_aliases.unwrap_or_default(),
    }
}

/// Generates a named `fn` returning `CompareOptions` with `report_all = true`
/// plus the listed override fields set to `true`. Replaces the dozen
/// single-purpose builders the test suite previously open-coded.
macro_rules! opts_fn {
    ($name:ident $(, $f:ident)* $(,)?) => {
        pub(crate) fn $name() -> CompareOptions {
            build_opts(OptsOverrides {
                report_all: true,
                $($f: true,)*
                ..OptsOverrides::default()
            })
        }
    };
}

opts_fn!(opts_report_all);
opts_fn!(tier1_opts, array_first_tier1);
opts_fn!(tier2_opts, array_first_tier1, array_first_tier2);
opts_fn!(cache_alias_opts, cache_alias);
opts_fn!(cache_alias_plus_tier1_opts, cache_alias, array_first_tier1);
opts_fn!(widening_opts, nullish_widening);
opts_fn!(
    widening_plus_swap_opts,
    nullish_widening,
    null_undefined_swap
);
opts_fn!(
    widening_plus_tier1_opts,
    nullish_widening,
    array_first_tier1
);
opts_fn!(
    widening_plus_cache_alias_opts,
    nullish_widening,
    cache_alias
);
opts_fn!(iife_async_opts, iife_async_wrapper);
opts_fn!(
    iife_async_plus_cache_alias_opts,
    iife_async_wrapper,
    cache_alias
);
opts_fn!(transient_cache_opts, transient_cache_wrap, cache_alias);
opts_fn!(
    transient_cache_plus_tier1_opts,
    transient_cache_wrap,
    cache_alias,
    array_first_tier1,
    nullish_widening
);
opts_fn!(request_narrowing_opts, request_field_narrowing);
opts_fn!(async_propagation_opts, async_propagation);
opts_fn!(defensive_guard_opts, defensive_null_guard);
opts_fn!(non_null_alias_opts, non_null_alias_local, cache_alias,);
opts_fn!(
    non_null_alias_plus_defensive_guard_opts,
    non_null_alias_local,
    defensive_null_guard,
    cache_alias,
);
opts_fn!(defensive_log_guard_opts, defensive_log_guard);
opts_fn!(
    dead_defensive_optional_chain_opts,
    dead_defensive_optional_chain_removal,
);
opts_fn!(
    dead_defensive_plus_cache_alias_opts,
    dead_defensive_optional_chain_removal,
    cache_alias,
);
opts_fn!(unknown_catch_narrowing_opts, unknown_catch_narrowing);
opts_fn!(
    promise_settled_discrimination_opts,
    promise_settled_discrimination,
);
opts_fn!(
    promise_settled_plus_non_null_alias_opts,
    promise_settled_discrimination,
    non_null_alias_local,
    cache_alias,
);

pub(crate) fn defensive_log_guard_custom_methods_opts(methods: Vec<String>) -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        defensive_log_guard: true,
        defensive_log_guard_methods: Some(methods),
        ..OptsOverrides::default()
    })
}

pub(crate) fn pure_narrowing_helper_opts(helpers: Vec<String>) -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(helpers),
        helper_call_site_substitution: true,
        destructure_then_narrow: true,
        ..OptsOverrides::default()
    })
}

/// Like [`pure_narrowing_helper_opts`] but with the v0.1.14 rule extensions
/// (`helper_call_site_substitution`, `destructure_then_narrow`) explicitly
/// disabled. Used by tests that prove the new rules are independently gated.
pub(crate) fn pure_narrowing_helper_strict_opts(helpers: Vec<String>) -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        pure_narrowing_helper: true,
        narrowing_helpers: Some(helpers),
        ..OptsOverrides::default()
    })
}

pub(crate) fn opts() -> CompareOptions {
    build_opts(OptsOverrides::default())
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
    expect_empty(&compare_exprs(base_expr, head_expr, opts));
}

pub(crate) fn assert_flagged(base_expr: &str, head_expr: &str, opts: &CompareOptions) {
    expect_nonempty(&compare_exprs(base_expr, head_expr, opts));
}

/// Like [`assert_equiv`] but feeds the inputs verbatim — no `let v = …`
/// wrapping. Use for class- or block-level fixtures.
pub(crate) fn assert_equiv_raw(base: &str, head: &str, opts: &CompareOptions) {
    expect_empty(&compare(base, head, opts));
}

pub(crate) fn assert_flagged_raw(base: &str, head: &str, opts: &CompareOptions) {
    expect_nonempty(&compare(base, head, opts));
}

fn expect_empty(findings: &[Finding]) {
    assert!(
        findings.is_empty(),
        "expected no findings, got: {:?}",
        findings
    );
}

fn expect_nonempty(findings: &[Finding]) {
    assert!(!findings.is_empty(), "expected divergence to be flagged");
}
