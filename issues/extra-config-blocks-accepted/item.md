---
created: 2026-04-09
updated: 2026-07-23
type: improvement
reporter: ai-review
assignee: jari
status: obsolete
priority: low
slug: extra-config-blocks-accepted
closed: 2026-07-23
---

# Extra config blocks for wrong section type silently accepted

_Source: `src/spec/validate.rs`_

## Description

A section with `type: table` can also include `chart`, `stats`, `list`, `markdown` config blocks, and validation only checks the required block for the declared type. Irrelevant blocks are silently accepted.

Example that passes validation:
```yaml
- type: table
  table: { columns: [...] }
  chart: { mark: bar, encoding: {} }
```

## Found by

Codex (gpt-5.4) during plan review, round 2.

## Fix

Add a section-level check in `validate()` that rejects config blocks that don't match the section type.
