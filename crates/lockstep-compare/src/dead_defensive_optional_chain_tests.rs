use crate::compare_options::CompareOptions;
use crate::test_helpers::{
    assert_equiv_raw, assert_flagged_raw, build_opts, dead_defensive_optional_chain_opts,
    dead_defensive_plus_cache_alias_opts, opts_report_all, OptsOverrides,
};

/// Wraps `base_body` and `head_body` in identical function shells and asserts
/// equivalence under the dead-defensive rule. Used to reduce the
/// boilerplate shared across the positive cases.
fn assert_equiv_function(signature: &str, base_body: &str, head_body: &str) {
    let (base, head) = function_pair(signature, base_body, head_body);
    assert_equiv_raw(&base, &head, &dead_defensive_optional_chain_opts());
}

/// Mirror of [`assert_equiv_function`] for negative cases.
fn assert_flagged_function(signature: &str, base_body: &str, head_body: &str) {
    assert_flagged_function_with(
        signature,
        base_body,
        head_body,
        &dead_defensive_optional_chain_opts(),
    );
}

fn assert_flagged_function_with(
    signature: &str,
    base_body: &str,
    head_body: &str,
    opts: &CompareOptions,
) {
    let (base, head) = function_pair(signature, base_body, head_body);
    assert_flagged_raw(&base, &head, opts);
}

fn function_pair(signature: &str, base_body: &str, head_body: &str) -> (String, String) {
    let base = format!("function f({signature}) {{\n{base_body}\n}}");
    let head = format!("function f({signature}) {{\n{head_body}\n}}");
    (base, head)
}

/// Positive: the canonical `if (!o?.p) { o.p = …; }` shape.
#[test]
fn dead_defensive_optional_chain_absorbs_negated_condition() {
    assert_equiv_function(
        "subscription, userUuid, plan",
        "if (!subscription?.userUuid) {\n\
                subscription.userUuid = userUuid;\n\
                subscription.plan = plan;\n\
            }",
        "if (!subscription.userUuid) {\n\
                subscription.userUuid = userUuid;\n\
                subscription.plan = plan;\n\
            }",
    );
}

/// Positive: positive (non-negated) condition with `Object.assign(o, …)`
/// witness.
#[test]
fn dead_defensive_optional_chain_absorbs_object_assign_witness() {
    assert_equiv_function(
        "o, x",
        "if (o?.p) { Object.assign(o, x); }",
        "if (o.p) { Object.assign(o, x); }",
    );
}

/// Positive: computed member assignment `o[i] = …`.
#[test]
fn dead_defensive_optional_chain_absorbs_subscript_write() {
    assert_equiv_function(
        "o, i, v",
        "if (!o?.p) { o[i] = v; }",
        "if (!o.p) { o[i] = v; }",
    );
}

/// Positive: augmented assignment counts as an unsafe write.
#[test]
fn dead_defensive_optional_chain_absorbs_augmented_assignment() {
    assert_equiv_function(
        "o, n",
        "if (!o?.p) { o.p += n; }",
        "if (!o.p) { o.p += n; }",
    );
}

/// Negative: reassignment of the target (not a member write) is not a
/// deadness witness.
#[test]
fn dead_defensive_optional_chain_rejects_reassignment() {
    assert_flagged_function(
        "o, newValue",
        "if (!o?.p) { o = newValue; }",
        "if (!o.p) { o = newValue; }",
    );
}

/// Negative: write nested inside an inner `if (o)` guard — the `?.` actively
/// protects the case where `o` is undefined.
#[test]
fn dead_defensive_optional_chain_rejects_guarded_write() {
    assert_flagged_function(
        "o, v",
        "if (!o?.p) { if (o) { o.p = v; } }",
        "if (!o.p) { if (o) { o.p = v; } }",
    );
}

/// Negative: pure read, no write — `?.` is meaningful (the function would
/// otherwise throw on the read).
#[test]
fn dead_defensive_optional_chain_rejects_read_only_body() {
    assert_flagged_function(
        "o, logger",
        "if (!o?.p) { logger.info(o); }",
        "if (!o.p) { logger.info(o); }",
    );
}

/// Negative: rule defaults off.
#[test]
fn dead_defensive_optional_chain_is_gated_off_by_default() {
    assert_flagged_function_with(
        "o, v",
        "if (!o?.p) { o.p = v; }",
        "if (!o.p) { o.p = v; }",
        &opts_report_all(),
    );
}

/// Composition: rule fires through the recursive walker — multiple chains
/// in the same function each resolve independently.
#[test]
fn dead_defensive_optional_chain_composes_in_recursive_walk() {
    assert_equiv_function(
        "a, b, v",
        "if (!a?.p) { a.p = v; }\nif (!b?.q) { b.q = v; }",
        "if (!a.p) { a.p = v; }\nif (!b.q) { b.q = v; }",
    );
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

fn log_consumer_opts(methods: &[&str]) -> CompareOptions {
    build_opts(OptsOverrides {
        report_all: true,
        dead_defensive_optional_chain_removal: true,
        dead_defensive_log_consumer_methods: Some(
            methods.iter().map(|s| String::from(*s)).collect(),
        ),
        ..OptsOverrides::default()
    })
}

enum Verdict {
    Equiv,
    Flagged,
}

/// Drives all log-consumer cases through one expectation helper so the
/// per-test boilerplate stays a single line.
fn check_log_consumer(
    methods: &[&str],
    signature: &str,
    base_body: &str,
    head_body: &str,
    verdict: Verdict,
) {
    let (base, head) = function_pair(signature, base_body, head_body);
    let opts = log_consumer_opts(methods);
    match verdict {
        Verdict::Equiv => assert_equiv_raw(&base, &head, &opts),
        Verdict::Flagged => assert_flagged_raw(&base, &head, &opts),
    }
}

macro_rules! log_consumer_case {
    ($name:ident, $verdict:expr, $methods:expr, $sig:expr, $base:expr, $head:expr) => {
        #[test]
        fn $name() {
            check_log_consumer(&$methods, $sig, $base, $head, $verdict);
        }
    };
}

log_consumer_case!(
    log_consumer_absorbs_optional_chain_in_logger_arg,
    Verdict::Equiv,
    ["logger.error"],
    "obj",
    "this.logger.error(`failed: ${obj?.id}`);",
    "this.logger.error(`failed: ${obj.id}`);"
);

log_consumer_case!(
    log_consumer_absorbs_when_first_arg_is_chained_value,
    Verdict::Equiv,
    ["logger.warn"],
    "obj",
    "logger.warn(obj?.id);",
    "logger.warn(obj.id);"
);

log_consumer_case!(
    log_consumer_absorbs_in_catch_block_template,
    Verdict::Equiv,
    ["logger.error"],
    "subscription",
    "try { run(); } catch (e) { this.logger.error(e, `Failed: ${subscription?.uuid}`); }",
    "try { run(); } catch (e) { this.logger.error(e, `Failed: ${subscription.uuid}`); }"
);

log_consumer_case!(
    log_consumer_allows_multiarg_logger,
    Verdict::Equiv,
    ["logger.error"],
    "err, obj",
    "this.logger.error(err, `${obj?.id}`);",
    "this.logger.error(err, `${obj.id}`);"
);

log_consumer_case!(
    log_consumer_rejects_when_allowlist_empty,
    Verdict::Flagged,
    [] as [&str; 0],
    "obj",
    "this.logger.error(`failed: ${obj?.id}`);",
    "this.logger.error(`failed: ${obj.id}`);"
);

log_consumer_case!(
    log_consumer_rejects_when_callee_not_in_allowlist,
    Verdict::Flagged,
    ["logger.warn"],
    "obj",
    "this.logger.error(`failed: ${obj?.id}`);",
    "this.logger.error(`failed: ${obj.id}`);"
);

log_consumer_case!(
    log_consumer_rejects_when_chained_value_wrapped_in_inner_call,
    Verdict::Flagged,
    ["logger.error"],
    "obj",
    "this.logger.error(recordAndReturn(obj?.id));",
    "this.logger.error(recordAndReturn(obj.id));"
);

log_consumer_case!(
    log_consumer_rejects_when_optional_chain_is_outside_arglist,
    Verdict::Flagged,
    ["logger.error"],
    "obj",
    "const x = obj?.id;\nthis.logger.error(`failed: x`);",
    "const x = obj.id;\nthis.logger.error(`failed: x`);"
);
