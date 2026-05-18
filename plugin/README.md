# lockstep — Claude Code plugin

Bundles lockstep into a Claude Code session: the MCP server, a skill the
agent auto-loads, a slash command, and a PostToolUse hook that turns
JS→TS-migration verification into a live signal during the edit loop.

## Install

The plugin needs the `lockstep` and `lockstep-mcp` binaries on PATH:

```
cargo install --path ../crates/lockstep-cli
cargo install --path ../crates/lockstep-mcp
```

From a Claude Code session:

```
/plugin marketplace add KyleBastien/lockstep
/plugin install lockstep@KyleBastien/lockstep
```

Restart the session to pick up hooks.

## What the plugin does

### MCP server

Registers `lockstep-mcp` as an MCP stdio server. Tools:

- `verify_migration` — args `{ paths?: string[], base?: string, repo?: string }`. Returns a structured `Report`.
- `explain_finding` — args `{ category: string }`.
- `get_config` — no args. Returns the resolved config.

### Skill (auto-loaded)

`lockstep-verify` — when to call `verify_migration`, how to read the report,
what each finding category means, and the standard remediation pattern.

### Slash command

`/lockstep-verify [paths...]` — call `verify_migration`, summarize verdict +
top three findings.

### Hook

`PostToolUse` (matcher `Edit|Write|MultiEdit`) — runs `lockstep verify`
after every edit, surfaces findings to the agent as informational output.
Non-blocking. Skipped when the cwd has no `.lockstep/` directory or when
`LOCKSTEP_DISABLE_POST_EDIT=1`.

## Disable the hook

```
export LOCKSTEP_DISABLE_POST_EDIT=1
```

Or delete `.lockstep/` in the repo.
