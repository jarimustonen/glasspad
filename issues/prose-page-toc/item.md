---
created: 2026-08-14
updated: 2026-08-14
type: feature
reporter: maintainer
status: done
priority: normal
commits:
- hash: 1baff4f
  summary: per-page H2/H3 TOC rail for prose spaces (approach a, server-side render)
- hash: af33886
  summary: apply 4-model review findings (collision-free slugs, break/footnote/text-less handling, PROSE_TEMPLATE const, CSS a11y)
closed: 2026-08-14
---

# Per-page TOC rail (on-this-page H2/H3 navigation) for prose spaces

_Source: producer-example docs/ port_

## Description

**Motivation:** producer-example's design docsite (port off build_docs.py → glasspad space, tracked in producer-example `project-view` epic) renders a right-hand per-page table of contents (H2/H3 of the current doc) alongside the left grouped nav. glasspad 0.8.0 has the grouped left sidebar (space-docsite-nav, done) but **no per-page TOC** — verified: no toc/on-this-page in src/artifact_host/shell.rs or base.css. Long spec pages (candidate.md ~80KB, decisions.md) are hard to navigate without it. **Ask:** prose template (and/or the shell) extracts the rendered page's H2/H3 and shows an 'on this page' rail (collapsible, hidden below a width breakpoint, like the grouped sidebar stacks). This is the last structural feature keeping build_docs.py alive for producer-example.

## Design decisions (implemented)

**Chosen approach: (a) — the rail lives inside the artifact's own prose fragment.**
Approach (b) (pass TOC to the shell as structured data) was rejected: it would add
shell/postMessage surface for zero benefit here. With (a) the TOC is derived
**server-side during markdown render** (`src/artifact_host/render.rs`) and emitted as
part of the artifact body, so in-page `#anchor` links resolve **natively inside the
null-origin sandbox** — no shell involvement, no new postMessage surface, no CSP
change. The security contract is byte-for-byte unchanged; `./test-security.sh` stays
green at 48 checks with no new probe needed (nothing new reaches the trusted chrome).

**Where it renders.** `render_to_body` routes the built-in `prose` template through a
new `render_prose_body` (all other templates — `dashboard`, client-supplied custom —
are the unchanged plain splice, byte-identical to pre-TOC). When a page has **≥2**
H2/H3 headings it emits `<div class="gp-doc"><article class="gp-prose">…</article><nav
class="gp-toc">…</nav></div>` — the rail is a **sibling** of `.gp-prose`, so the
"rendered blocks are direct children of `.gp-prose`" render contract is preserved. A
CSS grid (`base.css`) puts the article in a measure-capped left track and the rail in a
sticky 15rem right track; below a `60rem` breakpoint the grid collapses to one column
and the rail is `display:none` (matching how the grouped sidebar stacks away). The rail
is a native `<details open>` — **collapsible with zero JS**.

**Anchor-id scheme.** Every heading (H1–H6) gets a **server-generated** `id`:
slugify the heading's plain text (lowercase; each run of non-alphanumeric → single `-`;
trim leading/trailing `-`; Unicode letters kept so Finnish `ä`/`ö` stay readable; empty
→ `section`), then **deterministic collision disambiguation** (`-1`, `-2`, … in document
order). The id is written through pulldown-cmark's own HTML-escaping `Tag::Heading.id`
path — never an attacker-controlled raw id. Only H2/H3 populate the rail; the rest are
still deep-link targets.

**Security.** Heading text is artifact-derived/untrusted; it reaches the rail DOM only
**server-side HTML-escaped** (`&<>"'`), and the href is `#<slug>` (alphanumeric + `-`
only, also escaped). No `innerHTML`/raw-markup sink. Sandbox, CSP, `connect-src 'none'`,
`allow-*` set, and the return-channel airlock are all unchanged — the shell is not
touched at all. Graceful fallback: <2 H2/H3 (or a non-prose / full-document artifact)
renders the plain prose fragment, no empty rail (the fallback additionally stamps a
deep-link `id` on each heading — a deliberate, safe enhancement, not byte-identical).

## Review (4-model /llm-review, applied)

Reviewed by Gemini 3.1 Pro, GPT-5.6, Claude Opus 4.7, DeepSeek v4 — strong consensus.
Findings applied in `af33886` (report: `history/review-prose-page-toc.md`):

- **Critical (fixed):** `unique_slug` was NOT collision-free — a base-only counter handed
  the same disambiguated id to two headings (`## Setup / ## Setup / ## Setup 1` →
  `setup / setup-1 / setup-1`). `SlugSet` now reserves every emitted id → `setup /
  setup-1 / setup-1-1`.
- Soft/hard breaks in heading text → space (no glued words); text-less headings get an id
  but no blank rail entry; footnote-definition headings excluded from the rail; single
  `PROSE_TEMPLATE` const kills the string-dispatch drift risk; CSS a11y (sticky overflow,
  `scroll-margin-top`, `:focus-visible`, reduced-motion). Regression tests added for each.
- **Deferred (recorded):** nested `<ul>` TOC hierarchy for WCAG 1.3.1 (flat list + `nav`
  landmark is acceptable for a v1 convenience rail); full enum template dispatch (the
  const covers the real risk); streaming renderer (input bounded by `MAX_FILE_BYTES`);
  active-section highlight (needs JS/shell — out of scope by sandbox design).

Full gate green: `cargo fmt --all --check`, `cargo clippy --all-targets -D warnings`,
`cargo test`, `./test-security.sh` (48 checks). Security contract untouched.
