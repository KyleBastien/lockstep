---
name: lockstep-verify
description: |
  Use when the user is migrating a file from JavaScript to TypeScript, or after
  any edit to a `.ts` / `.tsx` file that has a `.js` counterpart on the default
  branch. Also use when interpreting lockstep findings or verdicts.

  This skill teaches WHEN to call `mcp__lockstep__verify_migration`, HOW to
  read its report, and the **remediation pattern** to recommend when the
  migration diverges from the JS baseline.
---

# lockstep — JS→TS migration syntax checker

`lockstep` is a deterministic gate that complements `tsc` and unit tests.
It strips TS-only constructs from the head version of a file, structurally
compares the result to the JS counterpart on the default branch, and surfaces
any divergence. The migration is "behavior-preserving" iff the report's
`verdict` is `approve`.

## When to call

Call `mcp__lockstep__verify_migration` (the slash command `/lockstep-verify`
is the same thing) in these situations:

- **After migrating any `.js` file to `.ts` / `.tsx`** — even if `tsc` passes.
  `tsc` validates types; lockstep validates that no logic changed.
- **Before committing migration work** — catch silent refactors that would
  ship to main.
- **When asked to "make this TypeScript"** — verify after the rewrite that the
  rewrite was annotation-only.
- **When debugging surprising behavior after a migration** — a passing verdict
  rules out syntactic drift as the cause.

Skip the call when:

- You authored a fresh `.ts` file with no `.js` predecessor — lockstep has
  nothing to compare against and will skip it.
- The user is making intentional behavioral changes alongside the migration
  (rare; in that case land the migration first as a separate commit/PR).

## Reading the report

The tool returns a `Report` with:

- `verdict.kind` — `approve` or `request_changes`.
- `summary` — one-line synopsis; read this first.
- `counts` — `{ error, warn, info }`.
- `findings[]` — each has:
  - `path` — head-side path.
  - `category` — see below.
  - `message` — human description.
  - `base_kind` / `head_kind` — tree-sitter node kinds at the divergence point.
  - `base_line` / `head_line` — 1-based.
  - `base_snippet` / `head_snippet` — ±2 lines of context.

### Finding categories

- **kind_mismatch** — AST nodes at the same position have different kinds.
  The migration changed the *shape* of the code. Revert the structural change.
- **token_mismatch** — Same kind, but leaf tokens differ. An identifier was
  renamed or a literal value changed. Restore the original names/values.
- **arity_mismatch** — Same kind, different number of children. An extra
  argument, branch, or statement was inserted (or removed). Reports should be
  granular; a root-level file mismatch usually means lockstep needs a bug fix.
- **dropped_statement** — A statement exists on one side but not the other
  after type-stripping. Restore the missing statement.
- **stripped_ts_construct** — Head uses a TS construct (enum, constructor
  parameter property) that lockstep won't mechanically equate to JS.
  Desugar the construct, or have the user opt in via `allow_enum_to_iife`
  in `.lockstep/config.toml`.
- **parse_error** — Stripped/normalized source failed to re-parse as JS.
  This is usually a bug in lockstep's stripper. Re-run with `--verbose` to
  dump the normalized source under `.lockstep/debug/`.

### Silent normalizations

These differences between base and head will **not** be flagged:

- `var` → `const` / `let` (both sides are normalized to `let`).
- Whitespace / formatting / trailing commas.
- Quote style (`'foo'` vs `"foo"`).
- TS-only constructs: type annotations, `as` casts, `!` assertions, generics,
  `interface` / `type` declarations, type-only imports, accessibility
  modifiers, `readonly` / `override` / `abstract` modifiers, and overload
  signatures.
- Constructor-assigned functions rewritten as class methods when names,
  params, async/generator flags, and bodies match.

Anything else means the runtime behavior of HEAD diverged from the baseline.

## Remediation pattern

When the verdict is `request_changes`, the standard fix is:

1. **Revert the divergent change** in the head file. Make the head a pure
   types-only annotation of the base file.
2. **Land the migration first** as its own commit / PR. Run lockstep again
   to confirm `approve`.
3. **Make the behavioral change as a follow-up**, where it can be reviewed on
   its own merits.

Do **not** silence findings by adding `@ts-ignore` or by opting in to allow
flags — that defeats the gate.

## Tool surface

- `mcp__lockstep__verify_migration` — args `{ paths?: string[], base?: string, repo?: string }`. Returns a `Report`.
- `mcp__lockstep__explain_finding` — args `{ category: string }`. Returns the prose for one finding category.
- `mcp__lockstep__get_config` — returns the resolved `.lockstep/config.toml`. Useful when verify produces unexpected results — confirm `default_branch`, `allow_*` flags.
