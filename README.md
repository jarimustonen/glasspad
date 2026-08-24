<p align="right">
  <img src="https://raw.githubusercontent.com/jarimustonen/glasspad/main/brand/logo.png" alt="Glasspad logo" width="140">
</p>

# glasspad

<!-- shipshape-readme:badges-start -->
[![CI](https://github.com/jarimustonen/glasspad/actions/workflows/ci.yml/badge.svg)](https://github.com/jarimustonen/glasspad/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/glasspad.svg)](https://crates.io/crates/glasspad)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
<!-- shipshape-readme:badges-end -->

AI-friendly scratchpad for rich visual views. A lightweight web service that
lets AI agents (Claude Code, OpenClaw, etc.) show dashboards, charts, and
interactive UIs to the user in their browser. Glasspad is actively developed,
pre-1.0 software released through crates.io, Homebrew, and GitHub Releases.

## Concept

Glasspad is an **HTML-artifact host**. The agent authors plain HTML; glasspad
serves it live and safely:

1. Point glasspad at a file or directory of HTML (or markdown) artifacts (`glasspad publish ./dir`)
2. Get back a loopback or hosted URL, according to the configured target
3. The user opens the URL; every artifact is sandboxed in a null-origin iframe

Each artifact is one HTML view (a **fragment** glasspad wraps in a themed shell,
or a **full document** served verbatim), addressed by a slug and linked to its
siblings with ordinary relative links. Under the default loopback target, editing
a file reloads the browser; the directory remains the single source of truth.

<p align="center">
  <img src="https://raw.githubusercontent.com/jarimustonen/glasspad/main/docs/assets/screenshot-status-dag.png" alt="A status DAG served as a Glasspad space" width="900">
</p>

## Installation

<!-- shipshape-readme:install-start -->
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
<!-- shipshape-readme:install-end -->

## Usage

```bash
glasspad publish ./myspace                    # publish markdown/HTML using the configured target
glasspad publish ./report.md --target hosted # override the target for one publish
glasspad loopback serve ./myspace            # run the live-reload server explicitly
glasspad build ./myspace ./out               # statically render a space (no server)
glasspad data ./old.csv                      # parse legacy CSV/JSON/mbox data to JSON rows
glasspad config show --json                  # inspect effective publish configuration
glasspad doctor --json                       # run read-only diagnostics
```

### Markdown-native spaces

Markdown files can sit alongside HTML in a space; `publish`, `loopback serve`, and
`build` render them through the built-in prose template or a template selected in
`glasspad.yaml`. They retain the same null-origin sandbox as HTML artifacts. For glossary
autolinks, cross-references, and custom semantic link styling, see
[Markdown preprocessing](docs/markdown-preprocessing.md).

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

## Security model

Every artifact renders in a null-origin sandboxed iframe under a strict Content
Security Policy. In particular, `connect-src 'none'` removes direct network
exfiltration channels; same-space navigation and theme updates pass through an
injected bridge instead. The boundary is enforced by the adversarial Playwright
regression suite in `./test-security.sh`. See [SECURITY.md](SECURITY.md) to report
a vulnerability.

## Documentation

- [Architecture](ARCHITECTURE.md)
- [Design system](DESIGN.md)
- [Markdown preprocessing](docs/markdown-preprocessing.md)
- [Examples](examples/)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)

## License

<!-- shipshape-readme:license-start -->
Licensed under the [MIT License](LICENSE).
<!-- shipshape-readme:license-end -->
