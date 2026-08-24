# Design brief: glasspad base template gallery

**Status: DRAFT — topic areas and goals pending maintainer review.**

This document is the hand-off package for an external design AI. It defines what to
design (a set of built-in space templates), the goals each must meet, the hard technical
contract, and the background material to read first. The design AI produces visual
designs / template fragments; it does not change glasspad's host code.

## Background — what a glasspad template is

glasspad hosts agent-authored HTML/markdown artifacts, each rendered inside a null-origin
sandboxed iframe. A **template** is a plain HTML *fragment* with a single `{{content}}`
slot. When a producer hands glasspad a markdown file, glasspad renders the markdown to
HTML blocks, splices them into the template's `{{content}}` slot, and wraps the result in
a themed shell that loads the base stylesheet (`/_gp/v1/base.css`, the `--gp-*` design
token system).

Today there are exactly two built-in templates, and they are minimal:

- `prose` — `<article class="gp-prose">{{content}}</article>` (the hardened reading
  theme, with an optional per-page table-of-contents rail).
- `dashboard` — `<div class="gp-card">{{content}}</div>` (just a card surface; not a
  real dashboard design).

A space selects its template in `glasspad.yaml` (`template: prose`, `template:
dashboard`, or a relative path to a custom fragment such as
`template: templates/report.html`). The goal of this work: ship a small gallery of
**designed** templates so that agent-produced spaces look genuinely good out of the box.

## Goals

1. **Beautiful with plain markdown.** The producer usually hands unstyled markdown —
   headings, paragraphs, lists, tables, code blocks, blockquotes, images. A template
   must look designed with exactly that input. It may *additionally* respond to
   documented optional classes (progressive enhancement), but must never require them.
2. **Token-native, dual-theme.** All colors, spacing, radii, and typography come from
   the `--gp-*` custom properties defined in `base.css` (documented in `DESIGN.md`).
   No new palettes, no hard-coded colors. Both shipped themes — Glass Light and Glass
   Dark — must work automatically, since the shell switches themes via
   `prefers-color-scheme` / an explicit toggle.
3. **Fully self-contained.** Artifacts render under a strict CSP in a null-origin
   iframe with `connect-src 'none'`: no external fonts, no CDN assets, no network
   fetches of any kind. System font stack only. Charts are available exclusively
   through the already-served `/_gp/v1/charts.js` (`gp.chart(el, spec)` over
   Vega-Lite).
4. **One file per template.** A template is a single HTML fragment (inline `<style>`,
   at most a few lines of inline JS if truly needed). No build step.
5. **Responsive and overflow-safe.** Wide content (tables, code, charts) scrolls inside
   its own container; the page never scrolls horizontally. Readable from phone width to
   wide desktop.
6. **Restrained, not decorative.** Match the existing design language (Linear/Notion/
   Vercel-inspired precision — see `DESIGN.md` §1): content is the hero, chrome is
   invisible, single accent color.

## Proposed template topic areas

Six templates, in priority order. Each entry: what it is for + design goals.

1. **`prose` (refresh)** — long-form reading: analyses, memos, articles,
   documentation pages. Already good; refine rather than redesign. Goals: typographic
   rhythm, comfortable measure, elegant tables/footnote-like metadata, keep the TOC-rail
   contract intact.
2. **`dashboard` (real design)** — metric/KPI boards with charts. Goals: a grid that
   auto-flows plain markdown sections into cards (e.g. each `h2` + following content
   becomes a card), stat emphasis for short "label: number" content, chart containers
   that size well.
3. **`report`** — the hybrid deliverable: narrative prose with embedded charts and
   tables, a title block (title/date/author from the first `h1` + following paragraph),
   and **print-friendly** styling (sensible page breaks, ink-light). This is the
   "business report / research deliverable" an agent hands a human.
4. **`board`** — status/progress views: project boards, agent-run status, task DAGs
   (see `examples/status-dag`). Goals: dense but scannable; lists render as columns or
   swimlane-like groups; strong use of the status color tokens.
5. **`index`** — a space's front page / navigation hub: a directory of sibling pages
   with short descriptions. Goals: link-list-as-content renders as a clean card
   directory; works as the first page a reader lands on in a multi-page space.
6. **`table`** — data-first pages dominated by one or a few large tables (catalogs,
   inventories, comparison matrices). Goals: full-width dense table styling, sticky
   header, zebra restraint, graceful horizontal scroll.

## Hard technical contract (must not be violated)

- Fragment only: no `<!DOCTYPE>`, `<html>`, `<head>`, `<body>` — the shell provides
  those and loads `base.css`. Exactly one `{{content}}` slot.
- The prose render contract: if a template wraps content in `.gp-prose`, rendered
  markdown blocks must be **direct children** of `.gp-prose` (the hardening and the
  TOC rail depend on it).
- CSP reality: no external requests will succeed; anything referenced must be inline
  or under `/_gp/v1/`.
- Templates receive whatever HTML the markdown renderer emits, including raw HTML
  passed through unsanitized — the sandbox, not the template, is the security
  boundary. Templates must not attempt to sanitize or trust content.
- Custom templates are selected per-space via `glasspad.yaml`; built-ins may be
  compiled into `crates/glasspad-core/src/artifact_host/render.rs`. The design
  deliverable is the fragment itself; wiring built-ins in is glasspad's work, not the
  design AI's.

## Background material to hand to the design AI

- `DESIGN.md` — the full design language (themes, tokens, component patterns).
- `crates/glasspad-cli/src/artifact_host/assets/base.css` — the actual `--gp-*`
  tokens and shipped component classes (129 tokens).
- This brief.
- `crates/glasspad-cli/src/skill.md` — the agent-facing operating guide (how
  producers actually author spaces).
- `examples/status-dag/` — a real example space (input for `board`).
- Representative markdown inputs: one long-form article, one metrics summary, one
  tabular catalog, one status listing — so every template is designed against real
  content. (To be assembled; can be synthesized from this repo's own docs.)
- Screenshots of the current `prose` and `dashboard` rendering in both themes, for
  the current-state baseline.

## Deliverable format expected from the design AI

Per template: one self-contained HTML fragment file (the template), a short usage note
(what content shapes it flatters, any optional enhancement classes), and ideally a
rendered preview per theme. Integration (compiling into built-ins, tests, docs) happens
in glasspad afterwards under this issue.
