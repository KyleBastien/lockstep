//! Configuration surface for the comparator.
//!
//! All equivalence rule flags exposed by `compare()` live here. The walker
//! itself lives in `walk.rs`; this module exists purely to keep that file
//! focused on the dispatch logic.

use std::collections::HashMap;
use std::path::PathBuf;

pub struct CompareOptions {
    pub path: PathBuf,
    /// If false, stop walking after the first divergence in a file.
    pub report_all: bool,
    /// Treat constructor assignments like `this.foo = function () {}` as
    /// equivalent to class methods named `foo` when their callable bodies match.
    pub allow_constructor_assigned_method_equivalence: bool,
    /// Treat matching constructor-local caches and instance fields as aliases.
    pub allow_closure_cache_field_alias: bool,
    /// Treat condition-guarded "first element or null" ternaries as equivalent
    /// to `EXPR[0] ?? null` (see `array_first_equivalence`).
    pub allow_array_first_element_or_null: bool,
    /// Additionally accept `EXPR[0] || null` and bare `EXPR[0]` as equivalent
    /// base shapes for `EXPR[0] ?? null` on head.
    pub allow_array_first_element_or_null_loose: bool,
    /// Treat `EXPR` ↔ `EXPR ?? null` (or `EXPR ?? undefined`) as equivalent at
    /// any AST position. Directional: head must be the widener.
    pub allow_nullish_widening: bool,
    /// Sub-flag of [`Self::allow_nullish_widening`]: additionally accept a bare
    /// `null` ↔ `undefined` literal swap at any position. Off by default even
    /// when widening is on, because `=== null` / `=== undefined` are
    /// observationally distinct.
    pub allow_null_undefined_swap: bool,
    /// Treat a sync head method whose body is `return (async () => BODY)();`
    /// as equivalent to a base async callable whose body is BODY. Lets TS
    /// migrations carry phantom-branded return types that block bare `async`.
    pub allow_iife_async_wrapper: bool,
    /// Accept the two-statement base pattern `CACHE = X; CACHE = unwrap(CACHE);`
    /// as equivalent to head `const LOCAL = X; CACHE = unwrap(LOCAL);` when
    /// LOCAL is a fresh local used only inside the unwrap. Composes with
    /// `allow_closure_cache_field_alias`.
    pub allow_transient_cache_wrap: bool,
    /// Accept a head `const IDENT = "PROP" in OBJ && typeof OBJ.PROP === T ? OBJ.PROP : undefined;`
    /// extraction, treating later `IDENT` uses as equivalent to base
    /// `OBJ.PROP` accesses in the same scope.
    pub allow_request_field_narrowing: bool,
    /// Accept head async + `await EXPR` where base is sync + bare `EXPR`,
    /// provided at least one new `await` appears on head. Directional: never
    /// the reverse.
    pub allow_async_propagation: bool,
    /// Accept a head-inserted `if (!CACHE) { logCall(...); return LIT; }`
    /// guard between two base statements. Observably changes behavior —
    /// stays off by default.
    pub allow_defensive_null_guard: bool,
    /// Accept a head-inserted `const LOCAL = CACHE;` extraction following a
    /// null guard for CACHE. Subsequent LOCAL uses then compare equal to base
    /// CACHE references for the duration of the block.
    pub allow_non_null_alias_local: bool,
    /// Accept a head-inserted `if (CACHE) { LOGGER.METHOD(CACHE, ...) }` wrap
    /// around an unconditional base logger call. Methods recognized as
    /// loggers are listed in `defensive_log_guard_methods`. Observably
    /// changes behavior when the logger has side effects on null inputs.
    pub allow_defensive_log_guard: bool,
    /// Method names treated as logger calls by `allow_defensive_log_guard`.
    /// Defaults to `["debug", "info", "warn", "error", "trace", "log"]`.
    pub defensive_log_guard_methods: Vec<String>,
    /// Accept a head-removed optional-chain (`OBJ?.PROP` → `OBJ.PROP`) when
    /// the enclosing block contains an unguarded write to `OBJ` that proves
    /// the chained object cannot be null/undefined at runtime in base. See
    /// `dead_defensive_optional_chain` for the deadness-witness rules.
    pub allow_dead_defensive_optional_chain_removal: bool,
    /// Dotted method paths recognized as log-only consumers by the
    /// `allow_dead_defensive_optional_chain_removal` log-consumer witness.
    /// When non-empty, a removed `?.` whose chained value lexically nests
    /// inside an argument list of a matching call is accepted as
    /// equivalent. Match shape: exact-suffix on callee compact text, with
    /// boundary on `.` or start-of-text. Empty list (default) leaves the
    /// rule's existing if-statement-only witness in force.
    pub dead_defensive_log_consumer_methods: Vec<String>,
    /// Accept a head-side `ERR instanceof Error ? ERR.PROP : <fallback>`
    /// ternary (inline or extracted into a `const`) as equivalent to a base
    /// bare `ERR.PROP` access, when `ERR` is bound by an enclosing
    /// `catch_clause`. TypeScript's strict-mode `unknown` catch binding forces
    /// this shape. See `unknown_catch_narrowing`.
    pub allow_unknown_catch_narrowing: bool,
    /// Accept a head-inserted `if (NAME.status !== "fulfilled") return X;`
    /// (or `!== "rejected"`) early-return guard before later `NAME.value.X`
    /// or `NAME.reason.X` access, when `NAME` is bound to a
    /// `Promise.allSettled` result. Deadness witness: base would have thrown
    /// reading `.value` on rejected (or `.reason` on fulfilled). See
    /// `promise_settled_discrimination`.
    pub allow_promise_settled_discrimination: bool,
    /// Accept head `HELPER(EXPR) ?? DEFAULT` as equivalent to base `EXPR`
    /// when `HELPER` is listed in `narrowing_helpers` and declared in head.
    /// Top-level helper declarations are filtered from the head program.
    /// Diverges observably when `EXPR` is not the expected runtime type.
    /// See `pure_narrowing_helper`.
    pub allow_pure_narrowing_helper: bool,
    /// Function names recognized as pure narrowing helpers by
    /// `allow_pure_narrowing_helper`. Defaults to empty — opt-in per workspace.
    pub narrowing_helpers: Vec<String>,
    /// Accept head `const LOCAL = HELPER(EXPR) ?? DEFAULT;` (or the
    /// type-predicate ternary form) followed by downstream reads of `LOCAL`,
    /// treating each `LOCAL` read as equivalent to base `EXPR`. Composes with
    /// `allow_pure_narrowing_helper`. See `helper_call_site_substitution`.
    pub allow_helper_call_site_substitution: bool,
    /// Accept the head shape `const { K: RAW_i, ... } = SRC;` followed by
    /// `const K_i = HELPER(RAW_i) ?? DEFAULT;` declarations, treating it as
    /// equivalent to base `const { K_i, ... } = SRC;`. Composes with
    /// `allow_pure_narrowing_helper`. See `destructure_then_narrow`.
    pub allow_destructure_then_narrow: bool,
    /// Map of array-returning narrowing helper name → field name on the
    /// base shape that the helper unwraps. When set and the head pattern
    /// `const RAW = SRC; const LOCAL = HELPER(RAW);` is observed, a
    /// scope-local alias `LOCAL` ↔ base `BASE_LOCAL.FIELD` is registered.
    /// See `helper_array_unwrap`.
    pub narrowing_helpers_unwrap: HashMap<String, String>,
    /// Map of zero-argument config-reader helper name → base expression
    /// path the helper reads. When set and the head pattern
    /// `const LOCAL = HELPER();` is observed, subsequent head accesses
    /// `LOCAL.X` compare equal to base `BASE_PATH.X`. See
    /// `helper_zero_arg_alias`.
    pub narrowing_helpers_aliases: HashMap<String, String>,
    /// Accept the composition of a registered zero-arg helper alias with
    /// an optional-chain removal AND a `??` widening to a safe-default
    /// literal at one AST position. Diverges observably when the aliased
    /// base path is actually nullish at runtime — see
    /// `alias_helper_widening`. Default OFF.
    pub allow_alias_helper_optional_chain_composition: bool,
}
