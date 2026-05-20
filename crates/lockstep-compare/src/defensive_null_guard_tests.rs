use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, defensive_guard_opts,
    non_null_alias_plus_defensive_guard_opts, opts_report_all,
};

const BASE_NO_GUARD: &str = "function f(cache, params) {
    if (!cache) { cache = load(); }
    Object.assign(cache, params);
}";

const HEAD_WITH_GUARD: &str = "function f(cache, params) {
    if (!cache) { cache = load(); }
    if (!cache) {
        logError(\"missing cache\");
        return false;
    }
    Object.assign(cache, params);
}";

#[test]
fn defensive_null_guard_absorbs_inserted_guard() {
    assert_equiv_raw(BASE_NO_GUARD, HEAD_WITH_GUARD, &defensive_guard_opts());
}

#[test]
fn defensive_null_guard_is_gated_off_by_default() {
    assert_flagged_raw(BASE_NO_GUARD, HEAD_WITH_GUARD, &opts_report_all());
}

#[test]
fn defensive_null_guard_directionality_base_has_guard_head_removed_flags() {
    assert_flagged_raw(HEAD_WITH_GUARD, BASE_NO_GUARD, &defensive_guard_opts());
}

#[test]
fn defensive_null_guard_rejects_extra_non_guard_statement() {
    let base = "function f(cache, params) { Object.assign(cache, params); }";
    let head = "function f(cache, params) {
        const meta = computeMeta();
        Object.assign(cache, params);
    }";
    assert_flagged_raw(base, head, &defensive_guard_opts());
}

#[test]
fn defensive_null_guard_rejects_guard_body_with_extra_work() {
    let base = "function f(cache, params) { Object.assign(cache, params); }";
    let head = "function f(cache, params) {
        if (!cache) {
            logError(\"missing\");
            mutateState();
            return false;
        }
        Object.assign(cache, params);
    }";
    assert_flagged_raw(base, head, &defensive_guard_opts());
}

#[test]
fn defensive_null_guard_rejects_two_extra_statements() {
    let base = "function f(cache, params) { Object.assign(cache, params); }";
    let head = "function f(cache, params) {
        if (!cache) {
            logError(\"missing\");
            return false;
        }
        const extra = computeExtra();
        Object.assign(cache, params);
    }";
    assert_flagged_raw(base, head, &defensive_guard_opts());
}

#[test]
fn defensive_null_guard_composes_with_non_null_alias_local() {
    let base = "function f(params) {
        Object.assign(cache, params);
        return cache;
    }";
    let head = "function f(params) {
        if (!cache) {
            logError(\"missing\");
            return false;
        }
        const current = cache;
        Object.assign(current, params);
        return current;
    }";
    assert_equiv_raw(base, head, &non_null_alias_plus_defensive_guard_opts());
}

#[test]
fn defensive_null_guard_alone_rejects_extra_const_local() {
    let base = "function f(params) {
        Object.assign(cache, params);
    }";
    let head = "function f(params) {
        if (!cache) {
            logError(\"missing\");
            return false;
        }
        const current = cache;
        Object.assign(current, params);
    }";
    assert_flagged_raw(base, head, &defensive_guard_opts());
}
