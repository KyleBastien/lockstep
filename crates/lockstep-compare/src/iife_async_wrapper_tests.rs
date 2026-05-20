use lockstep_core::Category;

use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, iife_async_opts, iife_async_plus_cache_alias_opts,
    opts_report_all,
};
use crate::walk::compare;

const BASE_ASYNC_ARROW: &str =
    "class C { constructor() { this.getInvoice = async (id) => { return id + 1; }; } }";
const HEAD_IIFE_SYNC_METHOD: &str =
    "class C { constructor() {} getInvoice(id) { return (async () => { return id + 1; })(); } }";

#[test]
fn iife_async_wrapper_absorbs_branded_return_pattern() {
    assert_equiv_raw(BASE_ASYNC_ARROW, HEAD_IIFE_SYNC_METHOD, &iife_async_opts());
}

#[test]
fn iife_async_wrapper_is_gated_off_by_default() {
    assert_flagged_raw(BASE_ASYNC_ARROW, HEAD_IIFE_SYNC_METHOD, &opts_report_all());
}

#[test]
fn iife_async_wrapper_rejects_sync_iife_callee() {
    let head =
        "class C { constructor() {} getInvoice(id) { return (() => { return id + 1; })(); } }";
    assert_flagged_raw(BASE_ASYNC_ARROW, head, &iife_async_opts());
}

#[test]
fn iife_async_wrapper_rejects_arguments_inside_iife_call() {
    let head = "class C { constructor() {} getInvoice(id) { return (async () => { return id + 1; })(id); } }";
    assert_flagged_raw(BASE_ASYNC_ARROW, head, &iife_async_opts());
}

#[test]
fn iife_async_wrapper_body_divergence_still_flagged() {
    let head = "class C { constructor() {} getInvoice(id) { return (async () => { return id - 1; })(); } }";
    let f = compare(BASE_ASYNC_ARROW, head, &iife_async_opts());
    assert!(!f.is_empty(), "body divergence must still flag");
    assert!(
        f.iter().any(|finding| matches!(
            finding.category,
            Category::TokenMismatch | Category::KindMismatch
        )),
        "expected token-level divergence, got: {:?}",
        f
    );
}

#[test]
fn iife_async_wrapper_directional_base_sync_head_async_flags() {
    let base = "class C { constructor() { this.f = (id) => { return id + 1; }; } }";
    let head = "class C { constructor() {} f(id) { return (async () => { return id + 1; })(); } }";
    assert_flagged_raw(base, head, &iife_async_opts());
}

#[test]
fn iife_async_wrapper_composes_with_cache_alias() {
    let base = "class C { constructor(c) { const invoiceCache = c; this.fetch = async (id) => { return invoiceCache[id]; }; } }";
    let head = "class C { constructor(c) { this._invoiceCache = c; } fetch(id) { return (async () => { return this._invoiceCache[id]; })(); } }";
    assert_equiv_raw(base, head, &iife_async_plus_cache_alias_opts());
}

#[test]
fn iife_async_wrapper_class_method_to_class_method_not_supported_yet() {
    let base = "class C { async getInvoice(id) { return id + 1; } }";
    let head = "class C { constructor() {} getInvoice(id) { return (async () => { return id + 1; })(); } }";
    assert_flagged_raw(base, head, &iife_async_opts());
}
