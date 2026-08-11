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

1. Point glasspad at a file or directory of HTML artifacts (`glasspad serve ./dir`)
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
glasspad serve ./myspace       # serve a directory of artifacts live
glasspad create ./report.html  # one-artifact space from a single file
glasspad build ./myspace ./out # statically render a space to HTML files (no server)
glasspad open myspace          # open it in the browser
glasspad publish ./report.html # publish one page to a hosted share server → /p/<slug>
glasspad publish-space ./docs  # publish a whole multi-page space → /p/<slug>/… (nav + relative links intact)
glasspad data ./old.csv        # parse a legacy CSV/JSON/mbox file to JSON rows
```

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

See [`src/skill.md`](src/skill.md) for the agent-facing guide and
[`DESIGN.md`](DESIGN.md) for the `--gp-*` design system that `base.css` provides.

## Status

🚧 Early development

## License

<!-- oss-readme:license-start -->
Licensed under the [MIT License](LICENSE).
<!-- oss-readme:license-end -->
