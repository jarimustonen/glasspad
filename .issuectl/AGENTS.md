# Agents policy (`.issuectl/AGENTS.md`)

How AI agents work this repository's issue tracker. The block between the
`issuectl-managed` sentinels at the bottom is regenerated from
`issues/.schema.yaml`, issuectl's built-in field defaults, and
`.issuectl/transitions.yaml` by `issuectl doctor --fix`;
the prose above it is hand-written and yours to improve. Project-wide guidance
(what an issue is for here, where plans live, how work is scheduled and handed off)
is in the top-level `AGENTS.md`; this file covers the tracker itself.

## What the tracker is for

`issues/<slug>/item.md` is the durable record of a piece of work: why it was opened,
what was decided, what landed, and how it ended. Closed issues stay in place, and
`ARCHITECTURE.md` points back to them for the reasons behind
decisions, so an issue is written for a reader a year from now, not just for the
agent picking it up next. Plans, analyses, and review findings that serve an issue
live beside its `item.md` as ordinary files (`plan.md`, `analysis.md`, `design.md`,
attachments); the CLI does not manage those and hand edits are fine.

## Why frontmatter goes through `issuectl`

The frontmatter is what everything else reads: `issuectl dag` derives the schedule
from `lane`, `lane_seq`, `collision`, and `blocked_by`; `list`, `stats`, and
`metrics` trust `status`, `updated`, and `closed`; the closing statuses require a
`closed:` date. The CLI takes the repo write lock, validates against the schema,
stamps `updated:` and `closed:`, and returns a version token that later writes can
pass as `--expected-version` when a stale write would matter. A hand edit gets none
of that and is the usual source of the inconsistencies `issuectl doctor` reports.
`issuectl <verb> --help` is the reference for each command; the recurring ones are
`set` and `update` for fields, `note` for comments and decisions, `check` for
checklist items, `close` for closing, and `update --patch-file` when one change
touches several fields and should land as one write.

Body markdown is different. The CLI writes only a few structured blocks
(`## Comments`, `## Decisions`, `## Agent Runs`, `## Resolution`), so editing the
rest of the body by hand, or with `issuectl body set`, is normal. The reserved
heading `## Notes` is legacy; comments go under `## Comments`.

The `/issue` skill is the worked reference for the CLI and its JSON shapes.
`issuectl skill install` writes the repo-local copies, so they are only as current
as the last install. If an installed copy talks about `issues/open/` directories,
numbered or not, it predates the flat slug layout this repo uses and should be
reinstalled with `issuectl skill install --force` (a plain install leaves existing
copies in place) rather than followed.

## What is particular about this repository

No transition rules are declared, so nothing mechanical stops an `untriaged` issue
from going straight to `done` or a closed one from reopening silently. The gate is
your judgment: a status should say what actually happened, `fixed` or `done` when
work landed, `wontfix`, `obsolete`, `duplicate`, or `cannot-reproduce` when it did
not, with the reason recorded (`close --comment`, or `intake reject` with its
disposition fields) so the next reader does not have to reconstruct it.

Incoming bug reports and feature requests arrive as `untriaged` and move through
`issuectl intake` (accept, defer, need-info, reject, cannot-reproduce, duplicate,
obsolete). Whether a
reported bug is fixed, deferred, or not a bug is Jari's call, as the top-level
`AGENTS.md` says; the tracker's job is to hold the report faithfully until that
call is made. `TODO.md` keeps the standing triage lessons, including the one about
review findings that describe no reachable failure: those close as `wontfix` rather
than being laned.

Scheduling is derived, not stored. `lane` is a serial queue whose head is the only
spawnable member, `collision` names a shared file that would make two lanes
conflict, `blocked_by` is the dependency edge, and `lane: unlaned` means "confirmed
independent", which differs from an absent lane meaning "not yet classified".
Labels classify content and are not a lifecycle or backlog state; a `deferred`
label in particular is a leftover from the pre-intake model. `issuectl dag --help`
and the issuectl repository's `docs/design/lane-design.md` explain the model.

Several worktrees write to `issues/` at once and each carries its own copy of the
tree, so the lock protects a single checkout, not the merge. Small, promptly
committed issue changes merge cleanly; a batch of status edits held on a branch for
a day does not. This is why the top-level policy asks for issue changes to be
committed as they are made.

## Commits and closing

Commits that serve an issue carry a `Refs-Issue: @<slug>` trailer; `issuectl
changelog` and `sync-commits` read those trailers, so the trailer is the link that
survives even when nobody records the hash on the issue. `close --stamp` adds a
`Fixes-Issue` trailer by rewriting HEAD's message, which changes its SHA, so it
belongs between committing the fix and pushing it, never after.

A closed issue is complete when its outcome is legible without the commit log: the
resolution or decision noted, the landing commit recorded or trailered, and the
acceptance criteria or done criteria reflecting what was actually verified
(`issuectl ready` reads the checklist). `issuectl doctor` before committing catches
what the schema can: missing `closed:` dates, dangling slug references after a
rename, and a managed block in this file that has drifted from the schema. Its
`--fix` regenerates that block and migrates legacy values; review the diff it makes
before committing it, since it rewrites files you did not touch.

<!-- issuectl-managed:start -->

<!-- issuectl-managed:format=1 -->

## Schema-derived rules (generated)

_Regenerated by `issuectl doctor --fix`. Do not hand-edit between the sentinels._

### Frontmatter fields

- `assignee` (optional, scalar)
- `blocked_by` (optional, list)
- `closed` (optional, scalar)
- `closed_by` (optional, scalar)
- `collision` (optional, list)
- `created` (optional, scalar)
- `deferred_until` (optional, scalar)
- `disposition_note` (optional, scalar)
- `disposition_reason` (optional, scalar) — allowed: by-design, out-of-scope, wontfix, withdrawn, superseded
- `duplicate_of` (optional, scalar)
- `epic` (optional, scalar)
- `labels` (optional, list)
- `lane` (optional, scalar)
- `owner` (optional, scalar)
- `priority` (required, scalar) — allowed: low, normal, medium, high
- `provenance` (optional, scalar)
- `provenance_detail` (optional, scalar)
- `related` (optional, list)
- `reporter` (optional, scalar)
- `review_status` (optional, scalar) — allowed: requested, in-review, approved, changes-requested
- `reviewer` (optional, scalar)
- `size` (optional, scalar) — allowed: S, M, L, XL
- `slug` (optional, scalar)
- `source_ref` (optional, scalar)
- `status` (required, scalar) — allowed: open, in-progress, testing, untriaged, deferred, needs-info, done, fixed, wontfix, duplicate, cannot-reproduce, obsolete
- `type` (required, scalar) — allowed: bug, task, feature, improvement, chore, decision, epic
- `updated` (optional, scalar)

### Required body sections by issue type

_No per-type body-section requirements declared._

### Status-transition rules

_No transition rules declared (lenient default)._

<!-- issuectl-managed:end -->
