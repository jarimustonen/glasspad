---
created: 2026-07-24
updated: 2026-07-24
type: bug
status: fixed
priority: normal
commits:
- hash: '8695354'
  summary: honour --json on skill --install-claude (envelope + error contract + tests)
- hash: 3bd753a
  summary: apply llm-review findings (no-panic HOME, warnings[], atomic created, user-scope tests)
closed: 2026-07-24
---

# skill --install-claude ignores --json (emits plain text, not an envelope)

## Description

`glasspad skill --install-claude --json` (and `--install-claude --user --json`)
prints a human-readable line (`Installed skill to .claude/skills/glasspad/SKILL.md`)
to stdout instead of a stable, versioned JSON envelope. Every other `glasspad`
command honours the AI-first `--json` contract (machine-readable envelope on
stdout, errors to stderr); this path is the lone exception.

Observed 2026-07-24 while verifying the CLI surface for release readiness.

## Expected

With `--json`, emit a stable versioned envelope on stdout describing the
install — e.g. the resolved install path, scope (project vs user), whether it
was created or overwritten, and the skill/cli version. Non-`--json` behaviour
(the plain "Installed skill to …" line) stays unchanged. Error cases (e.g.
missing `.claude/` at project scope) should already route their message to
stderr; under `--json` they should follow the same error-envelope convention as
the rest of the CLI.

## Acceptance

- `glasspad skill --install-claude --json` prints a valid JSON envelope on
  stdout (no plain-text line), matching the shape/versioning of the other
  commands' envelopes.
- `--user` variant likewise.
- Non-`--json` output unchanged.
- Error path under `--json` emits a structured error (stderr), consistent with
  the CLI's existing error convention.
- Test coverage for the `--json` install output added.

