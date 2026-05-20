use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, cache_alias_opts, transient_cache_opts,
    transient_cache_plus_tier1_opts,
};

const BASE_TRANSIENT: &str = "class C {
    constructor(api) {
        const customerBillingCache = null;
        this.api = api;
        this.update = function() {
            customerBillingCache = this.api.find();
            customerBillingCache = customerBillingCache.data.length > 0 ? customerBillingCache.data[0] : null;
            return customerBillingCache;
        };
    }
}";

const HEAD_TRANSIENT: &str = "class C {
    _customerBillingCache = null;
    constructor(api) { this.api = api; }
    update() {
        const result = this.api.find();
        this._customerBillingCache = result.data.length > 0 ? result.data[0] : null;
        return this._customerBillingCache;
    }
}";

#[test]
fn transient_cache_wrap_absorbs_local_then_assign_pattern() {
    assert_equiv_raw(BASE_TRANSIENT, HEAD_TRANSIENT, &transient_cache_opts());
}

#[test]
fn transient_cache_wrap_is_gated_off_by_default() {
    assert_flagged_raw(BASE_TRANSIENT, HEAD_TRANSIENT, &cache_alias_opts());
}

#[test]
fn transient_cache_wrap_composes_with_array_first_and_widening() {
    let head_with_widening = "class C {
        _customerBillingCache = null;
        constructor(api) { this.api = api; }
        update() {
            const result = this.api.find();
            this._customerBillingCache = result.data[0] ?? null;
            return this._customerBillingCache;
        }
    }";
    assert_equiv_raw(
        BASE_TRANSIENT,
        head_with_widening,
        &transient_cache_plus_tier1_opts(),
    );
}

const SINGLE_STMT_BASE: &str = "class C {
    constructor(api) {
        const cache = null;
        this.api = api;
        this.update = function() {
            cache = this.api.find();
        };
    }
}";

const SINGLE_STMT_HEAD: &str = "class C {
    _cache = null;
    constructor(api) { this.api = api; }
    update() {
        const result = this.api.find();
    }
}";

const TWO_STMT_BASE_DIVERGE: &str = "class C {
    constructor(api) {
        const cache = null;
        this.api = api;
        this.update = function() {
            cache = this.api.findOne();
            cache = cache.data[0];
            return cache;
        };
    }
}";

const HEAD_VALUE_DIVERGE: &str = "class C {
    _cache = null;
    constructor(api) { this.api = api; }
    update() {
        const result = this.api.findTwo();
        this._cache = result.data[0];
        return this._cache;
    }
}";

const HEAD_LOCAL_LEAK: &str = "class C {
    _cache = null;
    constructor(api) { this.api = api; }
    update() {
        const result = this.api.find();
        this._cache = result.data[0];
        return result;
    }
}";

#[test]
fn transient_cache_wrap_rejection_cases() {
    let opts = transient_cache_opts();
    let cases: &[(&str, &str, &str)] = &[
        (
            "value divergence between base and head RHS",
            TWO_STMT_BASE_DIVERGE,
            HEAD_VALUE_DIVERGE,
        ),
        (
            "local leaks out of the wrap into a later head statement",
            TWO_STMT_BASE_DIVERGE,
            HEAD_LOCAL_LEAK,
        ),
        (
            "single-statement block cannot match the two-statement pattern",
            SINGLE_STMT_BASE,
            SINGLE_STMT_HEAD,
        ),
    ];
    for (label, base, head) in cases {
        let f = crate::walk::compare(base, head, &opts);
        assert!(!f.is_empty(), "{label}: expected divergence; got empty");
    }
}
