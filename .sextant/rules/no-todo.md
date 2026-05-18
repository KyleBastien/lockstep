---
id: project.no-todo
name: "No TODO comments"
description: "Avoid shipping TODO markers in production code."
severity: warn
category: style
scope: file
languages: [rust, python, go, java, typescript, tsx, javascript]
evaluator:
  type: regex
  pattern: "TODO"
enabled: true
tags: [style]
---

# No TODO comments

Flags any line containing the word `TODO`.

## Why

TODOs accumulate. Track work in your issue tracker instead, where it's
visible to the team and gets prioritized like everything else.

## Fixing

- Move the TODO into an issue and link the issue id in a comment.
- Or: do the work now, since you're already in this code.
