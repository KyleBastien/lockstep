---
description: Verify the current JS→TS migration is behavior-preserving by comparing each touched .ts file to its .js counterpart on the default branch.
argument-hint: "[paths...]"
allowed-tools: ["mcp__lockstep__verify_migration", "mcp__lockstep__explain_finding", "mcp__lockstep__get_config"]
---

Run lockstep on the migration in progress. Arguments: `$ARGUMENTS`

1. Call `mcp__lockstep__verify_migration`. If `$ARGUMENTS` is non-empty, split on whitespace and pass as `paths`. Otherwise call with no arguments (checks every touched `.ts`/`.tsx` in the repo).

2. Read the structured report from the response:
   - `verdict` — `approve` means the migration preserves syntax. `request_changes` means at least one divergence was found.
   - `counts` — `{ error, warn, info }`.
   - `findings` — each has `path`, `category`, `message`, and (when available) `base_snippet` + `head_snippet`.

3. Summarize:
   - State the verdict and the counts.
   - List the top three findings: severity, category, `file:line`, message.
   - If a category is unfamiliar, call `mcp__lockstep__explain_finding` with that category and include the prose.

4. If verdict is `request_changes`, do **not** apply fixes unless the user asks. Instead, suggest the most actionable next step. The standard remediation pattern: revert the divergent change, land the migration as types-only first, then make the behavioral change as a separate PR.
