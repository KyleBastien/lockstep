use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, non_null_alias_opts,
    non_null_alias_plus_defensive_guard_opts, opts_report_all,
};

const BASE_DIRECT: &str = "class C {
    constructor() {
        this._invoiceCache = null;
    }
    update(params) {
        if (!this._invoiceCache) {
            this._invoiceCache = load();
        }
        Object.assign(this._invoiceCache, params);
        return this._invoiceCache;
    }
}";

const HEAD_ALIASED: &str = "class C {
    constructor() {
        this._invoiceCache = null;
    }
    update(params) {
        if (!this._invoiceCache) {
            this._invoiceCache = load();
        }
        if (!this._invoiceCache) {
            logError(\"missing\");
            return false;
        }
        const current = this._invoiceCache;
        Object.assign(current, params);
        return current;
    }
}";

#[test]
fn non_null_alias_local_absorbs_extraction_with_guard() {
    assert_equiv_raw(
        BASE_DIRECT,
        HEAD_ALIASED,
        &non_null_alias_plus_defensive_guard_opts(),
    );
}

#[test]
fn non_null_alias_local_is_gated_off_by_default() {
    assert_flagged_raw(BASE_DIRECT, HEAD_ALIASED, &opts_report_all());
}

#[test]
fn non_null_alias_local_rejects_extraction_without_preceding_guard() {
    let base = "function f() {
        Object.assign(cache, params);
        return cache;
    }";
    let head = "function f() {
        const current = cache;
        Object.assign(current, params);
        return current;
    }";
    assert_flagged_raw(base, head, &non_null_alias_opts());
}

#[test]
fn non_null_alias_local_rejects_when_local_is_reassigned() {
    let base = "function f() {
        if (!cache) { return false; }
        Object.assign(cache, params);
        cache = null;
        return true;
    }";
    let head = "function f() {
        if (!cache) {
            logError(\"x\");
            return false;
        }
        const current = cache;
        Object.assign(current, params);
        current = null;
        return true;
    }";
    assert_flagged_raw(base, head, &non_null_alias_opts());
}

#[test]
fn non_null_alias_local_works_with_bare_identifier_cache() {
    let base = "function f(params) {
        Object.assign(cache, params);
        return cache;
    }";
    let head = "function f(params) {
        if (!cache) {
            logError(\"x\");
            return false;
        }
        const current = cache;
        Object.assign(current, params);
        return current;
    }";
    assert_equiv_raw(base, head, &non_null_alias_plus_defensive_guard_opts());
}

#[test]
fn non_null_alias_local_rejects_when_guard_lacks_terminator() {
    let base = "function f(params) {
        Object.assign(cache, params);
    }";
    let head = "function f(params) {
        if (!cache) {
            logError(\"x\");
        }
        const current = cache;
        Object.assign(current, params);
    }";
    assert_flagged_raw(base, head, &non_null_alias_opts());
}
