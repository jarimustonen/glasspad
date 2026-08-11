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

## [0.5.0] - 2026-08-11

Completes the artifact **return channel** with its two planned later increments —
multi-round dialogue and an SSE delivery transport — and teaches `glasspad skill
install` to dual-home its companion skill under the pi.dev harness. The artifact
sandbox is unchanged throughout: every round stays a null-origin sandboxed iframe
with `connect-src 'none'`; the round-trip runs through the trusted shell + server.

### Added

- **Multi-round return channel (B2).** After an artifact calls `gp.submit()`, the
  creating agent can re-render the artifact **in place** and the user acts again — a
  conversational UI in one hosted page — via an owner-authenticated round push over
  the shell's live-reload stream. Each submission is bound to the content-version /
  round it answered: a stale-round submit is rejected (`409`), and every new round is
  re-verified to keep `connect-src 'none'` with no new sandbox grant (airlock held).
- **SSE transport for `await-submission` (A2).** A new
  `GET /api/v1/pages/<slug>/submissions/stream` endpoint pushes each submission to a
  held `EventSource`, with `since=<id>` cursor semantics (no re-deliver, no skip) and
  per-tenant isolation (a cross-tenant stream is refused with an opaque `404`). The
  backgrounded long-poll remains the default surface; SSE is the opt-in transport for
  watching many pages at once or sub-second streaming.
- **Dual-home skill install for pi.dev.** `glasspad skill install` now writes each
  skill's `SKILL.md` into `~/.pi/agent/skills/<name>/` in addition to the Claude Code
  path, so the companion skill is discoverable under the pi.dev harness
  (`/skill:name`). Idempotent and vendored-filtering-aware; the Claude Code install
  path is unchanged.

## [0.4.0] - 2026-08-10

Interactive artifacts can now **return user input to the agent that created
them** — closing the loop from a hosted or loopback page back to the calling
agent. The artifact sandbox itself is unchanged: the round-trip runs entirely
through the trusted shell + server, never by loosening the artifact's frozen
null-origin CSP.

### Added

- **`gp.submit()` return channel.** An artifact calls `gp.submit(data)` (or
  submits a native `<form>`, which the bridge intercepts) to hand a payload to
  the trusted shell, which relays it through the server to the creating agent —
  the artifact's own sandbox never gains network or form capability. Works in
  both hosted (`/p/` pages, API-key + per-tenant scoped) and loopback
  (`serve`, Origin-gated) modes.
- **Submit / poll / long-poll endpoints** under `_gp` (hosted and loopback),
  backed by a durable per-key submission store with a server-side long-poll
  primitive.
- **`glasspad await-submission`** — a backgrounded long-poll CLI command the
  creating agent uses to read the returned input.

### Changed

- The security regression suite grew to **48 browser checks** (+7 covering the
  return channel) plus the Wave 2a space-model probes. The artifact sandbox
  stays frozen — `connect-src 'none'`, no `allow-forms` — and that freeze is now
  regression-asserted.

## [0.3.1] - 2026-08-10

### Fixed

- **Hosted `/p/` pages: a link to another hosted page no longer shows "refused to
  connect".** A link inside a fragment-wrapped artifact navigated *within* the
  null-origin sandboxed iframe to the target page's shell, which is served
  `x-frame-options: DENY` + CSP `frame-ancestors 'none'` — so the browser refused to
  frame it. Wrapped fragments now carry `<base target="_top">`, so an inter-page link
  breaks out to the top-level tab (permitted by the content iframe's existing
  `allow-top-navigation-by-user-activation` sandbox flag) and loads normally. Sandbox
  isolation is unchanged: `<base>` has no `href` (nothing for `base-uri 'none'` to
  restrict, subresource URLs untouched), it grants the artifact no capability it did
  not already have (a full document could always author `target="_top"` links), and the
  bridge's same-space in-place swap still works (it reads each anchor's own `target`
  attribute, which `<base>` never sets).

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
