# Issues

Epics, tasks, and bugs tracked in the glasspad project.

## Structure

Each issue lives in `open/NN-short-title/` (or `closed/` when done).

- `item.md` — issue description (required)
- `plan.md`, `analysis.md`, `design.md` — optional deeper docs
- Screenshots in AVIF format

## Issue Types

- **epic** — high-level deliverable grouping related tasks. Has sub-issues table.
- **task** — single actionable work item. May have `parent` linking to an epic.
- **bug** — something broken. Has reproduction steps. Status includes `fixed` → `testing`.

## Frontmatter Fields

- **type** (required): `epic`, `task`, or `bug`
- **created** (required): date created (YYYY-MM-DD)
- **updated** (required): date of last update (YYYY-MM-DD)
- **reporter** (required): who reported the issue
- **assignee** (required): who is currently responsible
- **status** (required): current status (see workflow below)
- **priority** (required): `normal` or `high`
- **parent** (optional): parent epic number (for tasks and bugs)
- **commits** (optional): list of related commits (hash + summary)

## Templates

### Epic

```markdown
---
type: epic
created: YYYY-MM-DD
updated: YYYY-MM-DD
reporter: username
assignee: username
status: open | in-progress | done | closed
priority: normal | high
---

# NN. Epic title

## Description

What this epic delivers and why.

## Sub-issues

| #  | Title           | Status |
| -- | --------------- | ------ |
| 02 | Server setup    | open   |

## Acceptance Criteria

- [ ] Criterion 1
```

### Task

```markdown
---
type: task
created: YYYY-MM-DD
updated: YYYY-MM-DD
reporter: username
assignee: username
status: open | in-progress | done | closed
priority: normal | high
parent: NN
---

# NN. Task title

_Scope: what system/area this touches_

## Description

What needs to be done and why.

## Acceptance Criteria

- [ ] Criterion 1
```

### Bug

```markdown
---
type: bug
created: YYYY-MM-DD
updated: YYYY-MM-DD
reporter: username
assignee: username
status: open | fixed | testing | closed
priority: normal | high
parent: NN
---

# NN. Bug title

_Source: where it happens_

## Description

What's broken.

## Reproduction

Steps to reproduce.

## Quick Test

Command/URL to verify (optional — omit if not applicable).

## Screenshots

![description](filename.avif)
```

## Status Workflow

**Epics and tasks**: `open` → `in-progress` → `done` → `closed`

**Bugs**: `open` → `fixed` → `testing` → `closed`

When a bug is `fixed`, set `assignee` to the tester.

## Numbering

Sequential across `open/` and `closed/`. Zero-padded two digits (`01`, `14`).
Scan both directories to find the highest number before creating a new issue.
Never reuse numbers. Resolve conflicts from parallel work by renumbering newer issues.

## Workflow

1. Create with `/issue` skill
2. Add `plan.md` / `analysis.md` / `design.md` if needed
3. Update status as work progresses, add commits
4. When closed, move directory from `open/` to `closed/`
5. Keep epic sub-issue tables in sync with child issue statuses

## Planning Documents

All planning docs belong under an issue — this ties planning to trackable work.

- `plan.md` — architecture, implementation plans
- `analysis.md` — research and analysis
- `design.md` — design documents

## Images

All images must be AVIF. Convert PNG/JPG/WebP before adding.
