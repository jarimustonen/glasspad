# glasspad

<!-- oss-readme:badges-start -->
[![CI](https://github.com/jarimustonen/glasspad/actions/workflows/ci.yml/badge.svg)](https://github.com/jarimustonen/glasspad/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/glasspad.svg)](https://crates.io/crates/glasspad)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
<!-- oss-readme:badges-end -->

AI-friendly scratchpad for rich visual views. A lightweight, loopback-only web
service that lets AI agents (Claude Code, OpenClaw, etc.) show visual content —
dashboards, charts, interactive UIs — to the user in their browser.

## Concept

Glasspad is an **HTML-artifact host**. The agent authors plain HTML; glasspad
serves it live and safely:

1. Point glasspad at a file or directory of HTML (or markdown) artifacts (`glasspad serve ./dir`)
2. Get back a loopback URL
3. The user opens the URL; every artifact is sandboxed in a null-origin iframe

Each artifact is one HTML view (a **fragment** glasspad wraps in a themed shell,
or a **full document** served verbatim), addressed by a slug and linked to its
siblings with ordinary relative links. Edit a file and the browser reloads —
the directory is the single source of truth, so there is no upload/push step.

## Installation

<!-- oss-readme:install-start -->
**Homebrew** (macOS / Linux — the recommended cross-machine install):

```bash
brew install jarimustonen/glasspad/glasspad
```

**Prebuilt binaries** — download for your platform from the
[latest GitHub Release](https://github.com/jarimustonen/glasspad/releases/latest)
(each carries a checksum and build-provenance attestation), or via the release installer script.

**From crates.io** (builds from source):

```bash
cargo install glasspad
```
<!-- oss-readme:install-end -->

## Usage

```bash
glasspad serve ./myspace       # serve a directory of .html and/or .md artifacts live
glasspad create ./report.html  # one-artifact space from a single file
glasspad render ./doc.md        # render one markdown file through a template and serve it
glasspad build ./myspace ./out # statically render a space to HTML files (no server)
glasspad open myspace          # open it in the browser
glasspad publish ./report.html # publish one page to a hosted share server → /p/<slug>
glasspad publish-space ./docs  # publish a whole multi-page space → /p/<slug>/… (nav + relative links intact)
glasspad data ./old.csv        # parse a legacy CSV/JSON/mbox file to JSON rows
```

### Markdown-native spaces

A space can be a directory of **`.md`/`.markdown`** files just as well as `.html`:
`serve`, `build`, and `publish-space` render each markdown file server-side through a
built-in fragment template into a page (slug = filename stem), so a producer can hand
glasspad the markdown directly instead of pre-rendered HTML. `.md` and `.html` pages
coexist in one space — each file becomes a page keyed by its stem; a `.md` and `.html`
that share a stem is a hard collision (rename one). Pick the reading theme per-space in
`glasspad.yaml` with `template: prose` (the default), `template: dashboard`, or a relative
fragment path such as `template: templates/prose.html`. Single-file markdown accepts the
same custom-template seam via `glasspad render <file.md> --template <path>`. Rendered
markdown becomes an artifact in the **same** null-origin frozen sandbox as any other page — hostile HTML/script embedded
in the markdown cannot escape it or open an exfil channel (`connect-src 'none'` holds).

#### Producer preprocessing: glossary autolinks and cross-references

Glasspad deliberately does not infer document semantics. If a space needs glossary-term
autolinking, cross-reference resolution, citations, or similar transformations, preprocess
the markdown in the producer's build step and give glasspad the resulting space. Keep the
source and generated directories separate:

```text
docs-src/                         build/docs/
  guide.md          preprocessor   guide.md
  glossary.md       ───────────>   glossary.md
                                  glasspad.yaml
                                  templates/prose.html
```

For example, a producer can define `{{glossary:ledger|Ledger}}` as its own source notation
and turn this:

```markdown
A {{glossary:ledger|Ledger}} records the entries. See [Posting rules](posting.md).
```

into ordinary markdown plus an HTML link when a semantic class is needed:

```markdown
A <a class="xref glossary-term" href="glossary.html#ledger">Ledger</a> records the
entries. See [Posting rules](posting.md).
```

Then publish only the generated directory:

```bash
python build_docs.py docs-src build/docs

glasspad publish build/docs
```

Use a Markdown parser or another structure-aware transform in a real autolinker so it does
not rewrite terms inside code, existing links, or HTML. Keep link targets path-relative
(`posting.md` or `glossary.html#ledger`) for normal same-space navigation.

The renderer's class contract is intentionally simple. CommonMark links such as
`[Posting rules](posting.md)` render as plain `<a href="…">` elements and have no class
syntax. Raw HTML in markdown is passed through verbatim, without a sanitizer, so all
classes authored on an HTML link survive into the prose artifact. This is not a privileged
allowlist: the whole artifact remains untrusted inside the existing null-origin sandbox.
A producer can therefore style semantic links in a space template:

```yaml
# build/docs/glasspad.yaml
template: templates/prose.html
```

```html
<!-- build/docs/templates/prose.html -->
<style>
  .gp-prose a.xref { text-decoration-style: dotted; }
  .gp-prose a.glossary-term::after { content: " · glossary"; font-size: 0.75em; }
</style>
<article class="gp-prose">{{content}}</article>
```

The custom template remains an artifact fragment and may style against the `--gp-*`
tokens. Preprocessing decides which words and destinations have meaning; glasspad only
renders and hosts the links.

### Installing the companion skill

`glasspad skill` prints the agent-facing operating guide to stdout; `glasspad
skill --install` installs it as `SKILL.md` into an agent's skills directory
instead (`--install-claude` is a backward-compatible alias):

```bash
glasspad skill --install                 # install into ./.claude and ./.pi (project)
glasspad skill --install --user          # install into ~/.claude and ~/.pi/agent (home)
glasspad skill --install --agent claude  # Claude Code only
glasspad skill --install --agent pi      # pi.dev only (no ./.claude needed)
```

By default the install **dual-homes** the skill so it is discoverable under both
harnesses: Claude Code loads `<root>/.claude/skills/glasspad/SKILL.md`, and pi.dev
loads `~/.pi/agent/skills/glasspad/SKILL.md` (project scope: `./.pi/skills/…`),
invoking it as `/skill:glasspad`. `--agent {claude|pi|all}` selects the target(s)
(default: dual-home both); the install is idempotent, so re-running is always
safe. It refuses to overwrite a symlinked destination. Under `--json`, the success
envelope's `targets[]` array reports every path written (the top-level
`path`/`created` mirror the first target for backward compatibility). Targets are
written in order and the install is not transactional: if a later target fails,
an earlier one already written is left in place — re-run to complete it.

See [`crates/glasspad-cli/src/skill.md`](crates/glasspad-cli/src/skill.md) for the agent-facing guide and
[`DESIGN.md`](DESIGN.md) for the `--gp-*` design system that `base.css` provides.

## Status

🚧 Early development

## License

<!-- oss-readme:license-start -->
Licensed under the [MIT License](LICENSE).
<!-- oss-readme:license-end -->
