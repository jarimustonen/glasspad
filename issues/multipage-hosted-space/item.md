---
created: 2026-08-11
updated: 2026-08-11
type: feature
reporter: maintainer
status: done
priority: normal
commits:
- hash: 05899b6
  summary: design note for Gap 1 space ingest
- hash: 84de215
  summary: 'feat(hosted): space ingest core — bundle validator, spaces s'
- hash: 623544b
  summary: 'feat(cli): glasspad publish-space — publish a directory of .'
- hash: bdad225
  summary: 'test(security)+docs: Gap 1 space-ingest Wave + skill/README/'
- hash: 87ef73b
  summary: 'fix(hosted): apply llm-review — space crash-recovery, updated_at reten'
closed: 2026-08-11
---

# Multi-page hosted publish (space ingest) + markdown-native spaces

## Description

# Multi-page hosted publish (space ingest) + markdown-native spaces

## Use case
A producer repo (example-producer-cli) maintains a ~62-page spec/design **docsite** whose sources are
**markdown**. We want to host it on a `host-serve` instance (share.example.com) as a
browsable **multi-page** site with working in-space nav — ideally by handing glasspad the
**markdown directory** directly. Two capability gaps block this today (glasspad 0.3.1).

## Gap 1 — multi-page hosted publish (space ingest)
`publish <FILE>` is strictly single-file → one capability-slug page. `host-serve` ingests the
same per-file. There is **no way to publish/host a whole SPACE** (a directory of linked
artifacts) on the hosted server with the in-space **bridge nav + cross-page links** intact —
the internal links (`href="./other"` / `other.html`) don't resolve across separate per-file
publishes (each gets an independent `/p/<slug>`).
**Ask:** a way to publish/ingest a *space* into one hosted namespace (`/{space}/…`) with nav
+ relative links working, i.e. the `serve` experience but on `host-serve`. (An
`--idempotency-key`-style stable space slug so re-publish updates in place would be ideal.)

## Gap 2 — markdown-native spaces
`serve`/`build` scan **`.html` artifacts only** — a directory of `.md` renders **0 pages**
(tested with `build`). Only single-file `render <x.md> --template` handles markdown.
**Ask:** let `serve`/`build`/(and the space-publish above) treat a directory of `.md` as a
space — render each `.md` through a template into an artifact (slug = filename stem), so
producers can "just hand it the markdown." Per-space nav already exists via `glasspad.yaml`.

## Why it matters
Together these let a repo retire a bespoke HTML docsite generator and use glasspad as the
canonical tool: md sources → glasspad space → hosted multi-page site. Gap 1 alone already
unblocks hosting our current HTML bundle (glasspad 0.3.1 `build` already renders it — 62
pages — but only for local/offline use).

## Non-blocking parity notes (context, not required)
Our current generator also does autolinking / glossary cross-refs / section-TOC /
auto-discovered companion subtrees. We can keep a thin preprocessor for those; not asking
glasspad to own them. Flagging only so the template/space model leaves room for
producer-side preprocessing.

## Comments

### 2026-08-11T15:08:27Z · @orchestrator

Scope split for scheduling: THIS issue now tracks Gap 1 (multi-page hosted publish / space ingest — the unblocker). Gap 2 (markdown-native spaces) is split out to markdown-native-spaces, blocked_by this. A worktree here implements Gap 1 only.
