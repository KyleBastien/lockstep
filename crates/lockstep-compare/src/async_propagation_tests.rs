use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, async_propagation_opts, opts_report_all,
};

const BASE_SYNC_SPREAD: &str = "class C {
    execute() {
        const r = this.validate();
        return { ...r };
    }
}";

const HEAD_ASYNC_AWAIT: &str = "class C {
    async execute() {
        const r = await this.validate();
        return { ...r };
    }
}";

#[test]
fn async_propagation_absorbs_await_injection_in_method() {
    assert_equiv_raw(
        BASE_SYNC_SPREAD,
        HEAD_ASYNC_AWAIT,
        &async_propagation_opts(),
    );
}

#[test]
fn async_propagation_is_gated_off_by_default() {
    assert_flagged_raw(BASE_SYNC_SPREAD, HEAD_ASYNC_AWAIT, &opts_report_all());
}

#[test]
fn async_propagation_requires_at_least_one_await() {
    let base = "class C { execute() { return 1; } }";
    let head = "class C { async execute() { return 1; } }";
    assert_flagged_raw(base, head, &async_propagation_opts());
}

#[test]
fn async_propagation_directionality_base_async_head_sync_flags() {
    let base = "class C { async execute() { const r = await this.validate(); return { ...r }; } }";
    let head = "class C { execute() { const r = this.validate(); return { ...r }; } }";
    assert_flagged_raw(base, head, &async_propagation_opts());
}

#[test]
fn async_propagation_body_divergence_still_flagged() {
    let base = "class C { execute() { return this.validate(); } }";
    let head = "class C { async execute() { return await this.invalidate(); } }";
    assert_flagged_raw(base, head, &async_propagation_opts());
}

#[test]
fn async_propagation_works_for_constructor_assigned_base() {
    let base =
        "class C { constructor() { this.execute = function() { return this.validate(); }; } }";
    let head = "class C { constructor() {} async execute() { return await this.validate(); } }";
    assert_equiv_raw(base, head, &async_propagation_opts());
}
