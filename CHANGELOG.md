# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- oss-changelog:unreleased-start -->
## [Unreleased]

### Added
### Changed
### Fixed
<!-- oss-changelog:unreleased-end -->

## [0.3.0] - 2026-08-06

The agent→browser-HTML consolidation: glasspad becomes the single canonical surface
for turning agent-authored content into a hosted, shareable page. Adds a markdown +
reusable-template render path, a hosted public-share run mode, static build output,
and loopback process-management niceties — all on top of the frozen null-origin
sandbox / CSP security contract, which is unchanged.

### Added

- **`glasspad render <file.md> [--template prose|dashboard|./file.html]`** — server-side
  render of **markdown + a referenced reusable template** into a hosted artifact. The
  template governs only the artifact *body* (plugged into the `wrap.rs` fragment seam);
  glasspad keeps sole control of CSP / Trusted Types / nav / sandbox, so a custom template
  can never widen the security boundary. Default template is the hardened `prose` reading
  theme.
- **Hosted share-server run mode + `glasspad publish`** — a networked run mode beside the
  loopback server: API-key-authenticated ingest from many agents, public capability-slug
  URLs (`/p/<slug>`, `noindex`, 128-bit unguessable, "hold the link"), immutable pages with
  retention/GC (~90 days), and multi-tenant spaces. Artifact bodies are still served
  null-origin sandboxed — which is why public read is safe. Loopback `serve` is unchanged
  and keeps its DNS-rebinding Host guard; the public bind + auth live only in the new mode.
- **`glasspad build <space> <out>`** — static, self-contained render of a space to HTML
  (offline docsite / external preview transport), no server and no network bind. Reuses the
  same wrap seam as the server; self-contained by default (bundles the base libs), or
  `--shared-libs` to reference them.
- **`glasspad version`** now reports the build's short git commit SHA in its `--json`
  envelope (`data.commit`), falling back cleanly to `null` when built outside a git checkout
  (e.g. from a crates.io tarball). Complements the `glasspad version` / `-V` / `--version`
  command and its nested `{schema_version, data, warnings}` envelope.
- **Prose / reading theme** added to the `--gp-*` design system, hardened against arbitrary
  markdown HTML — the default look for rendered markdown.
- **Loopback process management** — `glasspad stop`, a `GLASSPAD_PORT` environment variable
  (explicit `--port` still wins), and a PID file at `~/.glasspad/server.pid` (atomic write,
  stale-PID detection, signal-based cleanup) so `stop` can find the running server.
- **Skill routing guidance** in the shipped agent skill (`glasspad skill install`) — when to
  reach for loopback `serve` vs `render` vs `publish` vs static `build`.

## [0.2.1] - 2026-08-05

Distribution-only release — no changes to glasspad itself; adds prebuilt-binary and
Homebrew install channels on top of 0.2.0.

### Added

- **Homebrew tap** — `brew install jarimustonen/glasspad/glasspad` (macOS / Linux), the
  recommended cross-machine install.
- **Prebuilt binaries** on every GitHub Release for macOS (Apple Silicon) and Linux
  (arm64, x86_64), with SHA-256 checksums and GitHub build-provenance attestations, plus a
  `curl`-to-shell installer script — all produced by `cargo-dist`.

## [0.2.0] - 2026-08-04

Initial public release. glasspad is a loopback-only **HTML-artifact host**: the
calling agent authors plain HTML in a directory and glasspad serves each file in a
null-origin sandboxed iframe.

### Added

- `glasspad serve <dir>` — serve a directory of HTML artifacts live; each renders in a
  null-origin sandboxed iframe with a themed shell, nav chrome, and auto-reload on edit.
- `glasspad create <file>` — build a one-artifact space from a single HTML file.
- `glasspad open <space>` — open a space in the browser.
- `glasspad data <file>` — parse a legacy CSV/JSON/mbox file to JSON rows.
- Base libraries served under `/_gp/v1/`: `base.css` (the `--gp-*` design system),
  `charts.js` (`gp.chart` over Vega-Lite), and `bridge.js` (same-space nav + theme).
- Security contract enforced by a self-contained Playwright suite (41 adversarial browser
  checks plus space-model probes: per-channel exfiltration, sandbox escape, direct-open,
  postMessage abuse, path traversal/symlink, injection, and Vega/eval).
