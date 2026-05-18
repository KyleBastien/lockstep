#!/usr/bin/env bash
# PostToolUse hook (matcher: Edit|Write|MultiEdit).
#
# Runs `lockstep verify` after edits and surfaces findings to the agent so
# it self-corrects without waiting to be asked. Non-blocking (always exits 0).
#
# Skipped when:
#   * `LOCKSTEP_DISABLE_POST_EDIT=1` is set,
#   * the `lockstep` binary is not on PATH,
#   * no `.lockstep/` directory exists in cwd (new repos opt in by running
#     `lockstep init`).

set -euo pipefail

if [ "${LOCKSTEP_DISABLE_POST_EDIT:-0}" = "1" ]; then
  exit 0
fi

if ! command -v lockstep >/dev/null 2>&1; then
  exit 0
fi

# Drain stdin so Claude isn't blocked on the pipe even if we ignore the payload.
cat >/dev/null 2>&1 || true

if [ ! -d .lockstep ]; then
  exit 0
fi

# Only run when at least one .ts or .tsx was touched. Probe via git diff
# against the configured default branch; if git fails, fall back to running
# unconditionally (cheap on a clean repo).
touched_ts=""
if command -v git >/dev/null 2>&1; then
  default_branch=$(awk -F'=' '/^[[:space:]]*default_branch[[:space:]]*=/ {gsub(/[" ]/, "", $2); print $2; exit}' .lockstep/config.toml 2>/dev/null || echo "main")
  touched_ts=$(git diff --name-only "${default_branch}"...HEAD 2>/dev/null | grep -E '\.(ts|tsx)$' || true)
  touched_unstaged=$(git diff --name-only 2>/dev/null | grep -E '\.(ts|tsx)$' || true)
  if [ -z "$touched_ts$touched_unstaged" ]; then
    exit 0
  fi
fi

report=$(mktemp)
trap 'rm -f "$report"' EXIT

if lockstep verify --format json >"$report" 2>/dev/null; then
  verdict=$(awk -F'"' '/"kind"/ {print $4; exit}' "$report" 2>/dev/null || echo "")
  if [ "$verdict" = "approve" ]; then
    exit 0
  fi
fi

# Surface the report to the agent as informational output. Non-blocking.
echo "lockstep found syntactic divergence between HEAD and the default branch:" >&2
cat "$report" >&2 || true
exit 0
