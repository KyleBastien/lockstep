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
6. Emit the first divergence with ±2-line snippets (or every divergence with `--report-all-findings`).

## What's silently allowed

- `var` → `const` / `let`.
- Whitespace / formatting / trailing commas.
- Quote style (`'foo'` vs `"foo"`).
- All TS-only syntax (the whole point of the migration).

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
report_all_findings = false
ignore = ["**/*.test.ts", "**/__snapshots__/**", ...]
```

## Known limitations (v1)

- Constructor parameter properties (`constructor(public x: T)`) emit a "desugar required" finding rather than being mechanically synthesized.
- Enums are rejected by default; opt in via `allow_enum_to_iife` after manual review.
- Identifier renames are not allowed.
- Cross-file moves (`git mv` + split into multiple `.ts` files) are not handled.

## License

MIT
