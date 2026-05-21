# lockstep

JS→TS migration syntax-equivalence checker. Verifies that `.ts` / `.tsx` files
on HEAD are behavior-preserving rewrites of their `.js` counterparts on the
default branch, by stripping TS-only constructs from HEAD and structurally
comparing the result to the JS baseline.

Complements `tsc` (which validates types) and unit tests (which validate
behavior at observed points): `lockstep` is the deterministic gate that
catches silent refactors slipping through during a migration.

## Install

```
cargo install --path crates/lockstep-cli
cargo install --path crates/lockstep-mcp
```

This puts `lockstep` and `lockstep-mcp` on PATH.

## Usage

```
cd <your-repo>
lockstep init                  # scaffold .lockstep/config.toml
lockstep verify                # check every touched .ts/.tsx
lockstep verify src/foo.ts     # check explicit paths
lockstep verify --base master  # override default branch
lockstep verify --format json
lockstep verify --verbose      # dump normalized sources to .lockstep/debug/
lockstep explain kind_mismatch # prose for a finding category
```

Exit codes: `0` clean, `1` findings at or above `--fail-on` (default `error`),
`2` tool error.

## File pairing

A file touched on HEAD is verified if EITHER:

1. Its `.js` / `.jsx` / `.mjs` / `.cjs` counterpart exists at the same stem on
   the default branch (the typical migration), OR
2. The same `.ts` / `.tsx` path exists on the default branch and contains a
   TS suppression marker (`@ts-ignore` or `@ts-nocheck`). Treat that earlier
   version as the JS-equivalent baseline by type-stripping it too — useful
   for re-migrations that remove `@ts-ignore`s or drop a file-level
   `@ts-nocheck`.

Files that don't match either case are skipped — they were authored fresh as TS
and have no JS baseline.

## Algorithm

For each `(base, head)` pair:

1. Read `base` from the default branch's tree (via `git2`).
2. Type-strip `head` (and `base` too in case 2): remove TS-only nodes — `type_annotation`, `as`/`satisfies`/`type_assertion`/`non_null_expression`, `interface_declaration`, `type_alias_declaration`, `ambient_declaration`, `type_arguments`, `type_parameters`, accessibility / `readonly` / `override` / `abstract` / `declare` modifiers, type-only imports, declared-only class fields, etc.
3. Normalize both sides: rewrite `var` → `let`, drop trailing commas.
4. Re-parse both as JavaScript.
5. Dual-walk the two trees: compare `kind()`, named-child arity, leaf-token text (with string/number canonicalization), skipping comments.
6. Emit granular divergences with ±2-line snippets, aligning nearby unchanged children so one root arity mismatch does not hide the actionable edits.

## What's silently allowed

- `var` → `const` / `let`.
- Whitespace / formatting / trailing commas.
- Quote style (`'foo'` vs `"foo"`).
- All TS-only syntax (the whole point of the migration), including overload
  signatures, type-only imports/exports, interfaces, and type aliases.
- Constructor-assigned functions rewritten as class methods when names, params,
  async/generator flags, and bodies match.

## What's flagged

- Renamed identifiers.
- Changed literal values.
- Re-ordered statements.
- Inserted or removed statements / arguments / branches.
- TS constructs that aren't trivially equivalent to JS (`enum`, constructor parameter properties) — by default. Opt in to enum-as-IIFE via `allow_enum_to_iife = true`.

## Claude Code plugin

The `plugin/` directory is a Claude Code plugin that bundles:

- An **MCP server** (`lockstep-mcp`) exposing `verify_migration`, `explain_finding`, and `get_config`.
- A **slash command** `/lockstep-verify` that calls `verify_migration` and summarizes the report.
- A **skill** `lockstep-verify` that teaches Claude (Codex, etc.) when to call the tool, how to read its findings, and the remediation pattern (revert divergent change → land migration types-only → make behavioral change as a separate PR).
- A **PostToolUse hook** that runs `lockstep verify` after every `Edit`/`Write`/`MultiEdit` and surfaces findings as informational output so the agent self-corrects mid-migration.

Install into a Claude Code session:

```
/plugin marketplace add KyleBastien/lockstep
/plugin install lockstep@KyleBastien/lockstep
```

Restart the session to pick up hooks.

## Workspace layout

```
crates/
  lockstep-core/        # Finding, Severity, Category, Verdict, Report
  lockstep-config/      # TOML loader + defaults
  lockstep-pairing/     # git2-backed pair discovery
  lockstep-strip/       # TS → JS-equivalent source rewrite
  lockstep-normalize/   # var → let, trailing-comma elision
  lockstep-compare/     # dual-walk AST comparator
  lockstep-engine/      # pipeline orchestration
  lockstep-cli/         # `lockstep` binary
  lockstep-mcp/         # `lockstep-mcp` stdio MCP server
plugin/                   # Claude Code plugin (commands + skills + hooks)
.lockstep/config.toml   # default config
```

## Code-quality gate (sextant)

This repo uses [sextant](https://github.com/kylebastien/sextant-mcp) as a
deterministic code-quality gate. Configuration lives in `.sextant/`. CI runs
`sextant grade --no-llm --fail-on warn` on every push and PR; the workspace is
kept at **0 errors / 0 warnings / 0 info** in lockstep with sextant's defaults
(see `.sextant/config.toml` — function-length, complexity, nesting, parameter
count, duplication, and pub-fn-untested thresholds).

Run locally:

```
cargo install --locked --git https://github.com/kylebastien/sextant-mcp --bin sextant sextant-cli
sextant grade --no-llm --fail-on warn
```

## Configuration

`.lockstep/config.toml`:

```toml
default_branch = "main"            # CLI: --base
allow_var_to_const_let = true
allow_formatting_diff = true
allow_enum_to_iife = false
allow_constructor_assigned_method_equivalence = true
allow_closure_cache_field_alias = false
allow_nullish_widening = false       # accept EXPR ↔ EXPR ?? null|undefined
allow_null_undefined_swap = false    # sub-flag: also accept bare null ↔ undefined
allow_iife_async_wrapper = false     # accept sync method that wraps async IIFE
allow_transient_cache_wrap = false   # accept `const LOCAL = X; CACHE = unwrap(LOCAL);` pattern
allow_request_field_narrowing = false # accept `const X = "p" in O && typeof O.p === "T" ? O.p : undefined;` extraction
allow_async_propagation = false      # accept sync→async + await injection (observable change)
allow_defensive_null_guard = false   # accept head-inserted `if (!cache) { log; return ERR; }` (observable change)
allow_non_null_alias_local = false   # accept head-inserted `const LOCAL = CACHE;` after a null guard
allow_defensive_log_guard = false    # accept head-inserted `if (cache) { LOGGER.METHOD(cache, ...) }` wrap
defensive_log_guard_methods = ["debug", "info", "warn", "error", "trace", "log"]
allow_dead_defensive_optional_chain_removal = false # accept head-removed `OBJ?.PROP` when block writes to OBJ prove the `?.` is dead
allow_unknown_catch_narrowing = false # accept `ERR instanceof Error ? ERR.PROP : <fallback>` ternary in catch blocks
allow_promise_settled_discrimination = false # accept head-inserted `status !== "fulfilled"` early-return guards
allow_pure_narrowing_helper = false # accept `HELPER(EXPR) ?? DEFAULT` ↔ base `EXPR` when HELPER ∈ narrowing_helpers
narrowing_helpers = []               # function names recognized by allow_pure_narrowing_helper
# v0.1.14: extracted-local + destructure-rename composition rules. Each cascades
# on automatically when `allow_pure_narrowing_helper = true`; set to `true`
# explicitly to enable in isolation.
allow_helper_call_site_substitution = false
allow_destructure_then_narrow = false
report_all_findings = true
ignore = ["**/*.test.ts", "**/__snapshots__/**", ...]
```

## Known limitations (v1)

- Constructor parameter properties (`constructor(public x: T)`) emit a "desugar required" finding rather than being mechanically synthesized.
- Enums are rejected by default; opt in via `allow_enum_to_iife` after manual review.
- Closure cache variables converted to instance fields are reported by default; opt in via `allow_closure_cache_field_alias = true` after manual review.
- Nullish widening (`EXPR` rewritten as `EXPR ?? null` or `EXPR ?? undefined` to satisfy a `T | null` field/return type) is reported by default; opt in via `allow_nullish_widening = true`. The rule is directional — head must be the widener.
- Bare `null` ↔ `undefined` literal swaps are gated separately on `allow_null_undefined_swap = true` (which itself requires `allow_nullish_widening`), because `=== null` / `=== undefined` are observationally distinct.
- Strict-TS structural reshapes are gated behind individual opt-in flags:
  - `allow_iife_async_wrapper` — head method returns `(async () => BODY)()` to satisfy a branded `Promise<X>` return type.
  - `allow_transient_cache_wrap` — head extracts a fresh local before assigning the narrowed cache field.
  - `allow_request_field_narrowing` — head extracts a narrowed local via `"prop" in obj && typeof obj.prop === "T" ? obj.prop : undefined`.
  - `allow_async_propagation` — head adds `async` + `await` because a subclass override widened the return type. Observable behavior change; default off.
  - `allow_defensive_null_guard` — head inserts `if (!cache) { logErr; return LITERAL; }` where base would have thrown. Observable behavior change; default off.
  - `allow_non_null_alias_local` — head extracts `const LOCAL = CACHE;` after a null guard so TS narrowing survives across `await` / method calls. Pure type-system artifact; default off.
  - `allow_defensive_log_guard` — head wraps a logger call in `if (CACHE) { LOGGER.METHOD(CACHE, ...) }`. Method names matched against `defensive_log_guard_methods` (default: `debug`, `info`, `warn`, `error`, `trace`, `log`). Observable behavior change if the logger has side effects on null inputs; default off.
  - `allow_dead_defensive_optional_chain_removal` — head drops a base `?.` (`OBJ?.PROP` → `OBJ.PROP`) when the enclosing `if`'s body unconditionally writes to `OBJ` (e.g. `OBJ.X = …`, `Object.assign(OBJ, …)`). The write would itself throw if `OBJ` were null/undefined, so the `?.` is dead defensive code. Error location differs at runtime; default off.
  - `allow_helper_call_site_substitution` — head extracts `const LOCAL = HELPER(EXPR) ?? DEFAULT;` (or the type-predicate ternary form) and reads `LOCAL` downstream. Each `LOCAL` read compares equal to base `EXPR`. Requires `HELPER` in `narrowing_helpers`. Cascades on with `allow_pure_narrowing_helper`. Observable when `EXPR` evaluates to a non-matching runtime type (same caveat as `allow_pure_narrowing_helper`).
  - `allow_destructure_then_narrow` — head destructures `{ K1: RAW_1, K2: RAW_2, … } = SRC` then declares `const K_i = HELPER(RAW_i) ?? DEFAULT;` for each binding, equivalent to base `{ K1, K2, … } = SRC`. Cascades on with `allow_pure_narrowing_helper`. Composes with `allow_helper_call_site_substitution` for downstream uses.
- Identifier renames are not allowed.
- Cross-file moves (`git mv` + split into multiple `.ts` files) are not handled.

## Patterns lockstep intentionally flags

A few migration shapes look mechanical but encode real runtime/behavior
changes. Lockstep keeps flagging these — manual review is the right answer:

- **Return-shape rewrite.** Head replaces `return RESULT;` with
  `if (RESULT.status !== "fulfilled") return X; const v = RESULT.value.PATH; return isPlainObject(v) ? v : {};`
  Base returned the raw `PromiseSettledResult`; head returns the inner
  payload. Callers see different shapes at runtime; the contract changed.
  No flag exists to absorb this — audit callers and split the behavioral
  change out of the migration.
- **Call-arg shape change.** Head replaces `f(obj)` with
  `f({ a: obj.a, b: obj.b })`. The callee may behave differently when
  given a projected object vs. the original; this is a refactor, not a
  strict-TS-required reshape.
- **Defensive deep spread.** Head replaces `f(x)` with
  `f({ ...x, props: { ...x.props } })`. Adds a copy that changes memory
  layout and the downstream-mutation contract — observably different from
  the base passthrough.

## License

MIT
