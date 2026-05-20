use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, build_opts, dead_defensive_optional_chain_opts,
    dead_defensive_plus_cache_alias_opts, opts_report_all, OptsOverrides,
};

/// Positive: the canonical PushPress shape — `if (!o?.p) { o.p = …; }`.
#[test]
fn dead_defensive_optional_chain_absorbs_negated_condition() {
    let base = "function f(subscription, userUuid, plan) {
        if (!subscription?.userUuid) {
            subscription.userUuid = userUuid;
            subscription.plan = plan;
        }
    }";
    let head = "function f(subscription, userUuid, plan) {
        if (!subscription.userUuid) {
            subscription.userUuid = userUuid;
            subscription.plan = plan;
        }
    }";
    assert_equiv_raw(base, head, &dead_defensive_optional_chain_opts());
}

/// Positive: positive (non-negated) condition with `Object.assign(o, …)`
/// witness.
#[test]
fn dead_defensive_optional_chain_absorbs_object_assign_witness() {
    let base = "function f(o, x) {
        if (o?.p) {
            Object.assign(o, x);
        }
    }";
    let head = "function f(o, x) {
        if (o.p) {
            Object.assign(o, x);
        }
    }";
    assert_equiv_raw(base, head, &dead_defensive_optional_chain_opts());
}

/// Positive: computed member assignment `o[i] = …`.
#[test]
fn dead_defensive_optional_chain_absorbs_subscript_write() {
    let base = "function f(o, i, v) {
        if (!o?.p) {
            o[i] = v;
        }
    }";
    let head = "function f(o, i, v) {
        if (!o.p) {
            o[i] = v;
        }
    }";
    assert_equiv_raw(base, head, &dead_defensive_optional_chain_opts());
}

/// Positive: augmented assignment counts as an unsafe write.
#[test]
fn dead_defensive_optional_chain_absorbs_augmented_assignment() {
    let base = "function f(o, n) {
        if (!o?.p) {
            o.p += n;
        }
    }";
    let head = "function f(o, n) {
        if (!o.p) {
            o.p += n;
        }
    }";
    assert_equiv_raw(base, head, &dead_defensive_optional_chain_opts());
}

/// Negative: reassignment of the target (not a member write) is not a
/// deadness witness.
#[test]
fn dead_defensive_optional_chain_rejects_reassignment() {
    let base = "function f(o, newValue) {
        if (!o?.p) {
            o = newValue;
        }
    }";
    let head = "function f(o, newValue) {
        if (!o.p) {
            o = newValue;
        }
    }";
    assert_flagged_raw(base, head, &dead_defensive_optional_chain_opts());
}

/// Negative: write nested inside an inner `if (o)` guard — the `?.` actively
/// protects the case where `o` is undefined.
#[test]
fn dead_defensive_optional_chain_rejects_guarded_write() {
    let base = "function f(o, v) {
        if (!o?.p) {
            if (o) {
                o.p = v;
            }
        }
    }";
    let head = "function f(o, v) {
        if (!o.p) {
            if (o) {
                o.p = v;
            }
        }
    }";
    assert_flagged_raw(base, head, &dead_defensive_optional_chain_opts());
}

/// Negative: pure read, no write — `?.` is meaningful (the function would
/// otherwise throw on the read).
#[test]
fn dead_defensive_optional_chain_rejects_read_only_body() {
    let base = "function f(o, logger) {
        if (!o?.p) {
            logger.info(o);
        }
    }";
    let head = "function f(o, logger) {
        if (!o.p) {
            logger.info(o);
        }
    }";
    assert_flagged_raw(base, head, &dead_defensive_optional_chain_opts());
}

/// Negative: rule defaults off.
#[test]
fn dead_defensive_optional_chain_is_gated_off_by_default() {
    let base = "function f(o, v) {
        if (!o?.p) {
            o.p = v;
        }
    }";
    let head = "function f(o, v) {
        if (!o.p) {
            o.p = v;
        }
    }";
    assert_flagged_raw(base, head, &opts_report_all());
}

/// Composition: rule fires through the recursive walker — multiple chains
/// in the same function each resolve independently.
#[test]
fn dead_defensive_optional_chain_composes_in_recursive_walk() {
    let base = "function f(a, b, v) {
        if (!a?.p) {
            a.p = v;
        }
        if (!b?.q) {
            b.q = v;
        }
    }";
    let head = "function f(a, b, v) {
        if (!a.p) {
            a.p = v;
        }
        if (!b.q) {
            b.q = v;
        }
    }";
    assert_equiv_raw(base, head, &dead_defensive_optional_chain_opts());
}

/// Identifier aliasing: base bare identifier ↔ head `this.PROP` via
/// `allow_closure_cache_field_alias`. Base uses a constructor-local cache
/// captured by an assigned function; head exposes the same value as a
/// class field accessed through `this`.
#[test]
fn dead_defensive_optional_chain_resolves_cache_field_alias() {
    let base = "class C {
        constructor(initial) {
            const cache = initial;
            this.update = function (params) {
                if (!cache?.p) {
                    cache.p = params;
                }
            };
        }
    }";
    let head = "class C {
        constructor(initial) {
            this._cache = initial;
        }
        update(params) {
            if (!this._cache.p) {
                this._cache.p = params;
            }
        }
    }";
    assert_equiv_raw(base, head, &dead_defensive_plus_cache_alias_opts());
}

/// Identifier aliasing: head `const local = this._cache;` extraction so later
/// head `local.PROP` ↔ base `this._cache.PROP`. Requires the defensive null
/// guard rule too, since the extraction is paired with a guard.
#[test]
fn dead_defensive_optional_chain_resolves_non_null_alias_local() {
    let base = "class C {
        update(params) {
            if (!this._cache) {
                this._cache = load();
            }
            if (!this._cache?.userUuid) {
                this._cache.userUuid = params.userUuid;
            }
        }
    }";
    let head = "class C {
        update(params) {
            if (!this._cache) {
                this._cache = load();
            }
            if (!this._cache) {
                logError(\"missing\");
                return false;
            }
            const local = this._cache;
            if (!local.userUuid) {
                local.userUuid = params.userUuid;
            }
        }
    }";
    let opts = build_opts(OptsOverrides {
        report_all: true,
        dead_defensive_optional_chain_removal: true,
        non_null_alias_local: true,
        defensive_null_guard: true,
        cache_alias: true,
        ..OptsOverrides::default()
    });
    assert_equiv_raw(base, head, &opts);
}
