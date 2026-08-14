---
created: 2026-08-14
updated: 2026-08-14
type: feature
reporter: jari
status: in-progress
priority: normal
commits:
- hash: 1baff4f
  summary: per-page H2/H3 TOC rail for prose spaces (approach a, server-side render)
---

# Per-page TOC rail (on-this-page H2/H3 navigation) for prose spaces

_Source: aggountant docs/ port_

## Description

**Motivation:** aggountant's design docsite (port off build_docs.py → glasspad space, tracked in aggountant `project-view` epic) renders a right-hand per-page table of contents (H2/H3 of the current doc) alongside the left grouped nav. glasspad 0.8.0 has the grouped left sidebar (space-docsite-nav, done) but **no per-page TOC** — verified: no toc/on-this-page in src/artifact_host/shell.rs or base.css. Long spec pages (candidate.md ~80KB, decisions.md) are hard to navigate without it. **Ask:** prose template (and/or the shell) extracts the rendered page's H2/H3 and shows an 'on this page' rail (collapsible, hidden below a width breakpoint, like the grouped sidebar stacks). This is the last structural feature keeping build_docs.py alive for aggountant.

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
renders exactly as before, no empty rail.
