# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- oss-changelog:unreleased-start -->
## [Unreleased]

### Added
- **`glasspad config path` / `glasspad config show`**: read-only inspection of the effective
  configuration and where each value came from (flag, environment variable, config file, or
  default), with a `--json` envelope. `api_key` is reported only as `<set>`/`<unset>` — the
  secret value is never printed by either output mode.
- **Custom templates for a whole space**: a space can now declare its own producer-supplied
  template and have every markdown page render through it, instead of being limited to the
  built-in `prose` / `dashboard` templates. Grouped sidebar navigation, the generated landing
  index, and the per-page TOC rail continue to work. A space that declares no template renders
  exactly as before.

### Changed
- **Hosted snapshots share page bodies behind `Arc`**: publish, update, and round-push no
  longer deep-copy every page and asset body while holding the mutation lock, making a
  snapshot clone proportional to the number of spaces rather than their total size.
  `MAX_PAGES` is now enforced on scan/load as well as on write.

### Fixed
<!-- oss-changelog:unreleased-end -->

## [0.14.0] - 2026-08-15

Hardens hosted-store crash consistency by moving mutable hosted spaces and live overlays to
immutable generation directories selected by an atomically replaced `current` pointer.

### Changed
- **Hosted spaces now commit through generation pointers**: each publish/update writes a complete
  immutable generation, fsyncs it, then flips a single `current` pointer. A crash before the flip
  keeps the previously served generation live, while a completed flip preserves the existing
  committed-vs-durable honesty contract.
- **Live overlays use the same generation model**: a failed or interrupted next round no longer
  discards the previously committed live round because `live.html` and metadata are selected as
  one generation.

### Fixed
- **Hosted-store recovery is more conservative and symlink-safe**: corrupt or unresolvable
  pointers no longer downgrade to stale legacy content, restored backups are reconciled on startup,
  page/space slug collisions fail closed, and legacy flat spaces plus two-file overlays still read
  transparently during upgrade.

## [0.13.0] - 2026-08-15

Fixes the hosted-store durability honesty gap found during the stable-URL update review.
A post-commit parent-directory fsync failure no longer leaves disk and the in-memory served
snapshot disagreeing about which space version is live.

### Fixed
- **Hosted space materialization is now commit-honest**: the atomic rename is treated as the
  commit point, and all callers distinguish a durable commit from an unconfirmed post-commit
  durability warning. Replace/create paths now swap the served snapshot whenever disk already
  contains the new tree, so a reported error cannot secretly mean "the next restart serves
  different content." Deterministic fault-injection tests pin the fsync-after-swap window.
- **Hosted atomic-publish paths share the same committed-vs-durable outcome handling** across
  `materialize_space`, page writes, live-round writes, stable `--space-key` publish, and
  `PUT /api/v1/spaces/{slug}` update. Review follow-up work for immutable generation
  directories + a current pointer is tracked as `hosted-store-generation-pointer`.


## [0.12.0] - 2026-08-15

Adds stable-URL hosted republishing: `glasspad publish --update <slug>` can replace a
published space in place while preserving the same `/p/<slug>` link. This release also
locks in grouped-sidebar behaviour for hosted spaces with a regression test; the observed
maalla.dev sidebar loss is a deploy/stored-metadata issue, not a current-code defect.

### Added
- **`glasspad publish --update <slug>`**: update an existing hosted space by capability
  slug, preserving the public URL for living documents such as meeting notes. The update
  path is owner-scoped, fails closed for missing or cross-tenant slugs, keeps existing
  idempotency-key semantics unchanged, and uses the same hosted-store durability pattern
  as space publish.
- **Hosted update API**: owner-authenticated `PUT /api/v1/spaces/{slug}` replaces a space
  snapshot in place, refreshes retention, and keeps the artifact sandbox/security contract
  unchanged.

### Fixed
- **Grouped hosted sidebar regression coverage**: added a test proving grouped nav chrome
  is present for every hosted page URL and survives store reopen. The current code already
  behaves correctly; affected maalla.dev spaces need the hosted server upgraded and then
  re-published so `nav_groups` is persisted.

## [0.11.0] - 2026-08-14

Fixes the **hosted artifact return channel** and adds **inline-SVG diagrams** to
markdown (prose) spaces. The artifact security contract is unchanged — the diagram
pattern adds no JavaScript or `eval` surface and requires zero change to the
null-origin sandbox or the artifact CSP; each page stays a null-origin sandboxed
iframe (`./test-security.sh` 48 checks + Wave 2a green).

### Added
- **Inline-SVG diagram pattern for markdown spaces** — a documented, supported way to
  embed theme-aware diagrams (the priority case: a colour-coded status DAG,
  done/next/blocked/future) in prose pages. The producing agent owns SVG generation and
  embeds it inline; glasspad supplies only theme-aware CSS — a `--gp-status-*` palette
  across all three theme blocks plus `.gp-diagram` / `.gp-node` / `.gp-edge` /
  `.gp-status-*` / `.gp-legend` / `.gp-chip` classes. Chosen over native mermaid because
  it directly serves the live project-view case and adds no new JS/eval surface.
- **`glasspad submissions <slug>` drain command** — a returning (or previously departed)
  agent can fetch the accumulated submission backlog for a published page (per-tenant
  scoped, `--json`, paginated; cross-tenant access → opaque 404).
- **`publish` return-channel discoverability** — publishing a page now prints the exact
  `await-submission` invocation (with the configured `--public-host`) plus a retention note.

### Fixed
- **Hosted return channel now works for CLI-published (space) pages** — hosted form
  submissions reach the creating agent end-to-end; this was a genuine defect in the
  hosted delivery path, not only a UX gap. Multi-page version binding, fail-closed owner
  checks, and paginated drain hardened after multi-model review.

## [0.10.0] - 2026-08-14

Adds a **per-page "on this page" table of contents** to prose (markdown) spaces —
the last structural docsite feature, so a grouped, navigable design docsite ports onto
glasspad without a bespoke generator. The artifact security contract is unchanged: the
rail lives inside the artifact's own fragment (no trusted-shell surface, no postMessage),
each page stays a null-origin sandboxed iframe (`./test-security.sh` 48 checks + Wave 2a
green).

### Added

- **Per-page TOC rail for prose spaces.** A markdown page with ≥2 H2/H3 headings now
  renders an "on this page" `<nav class="gp-toc">` alongside the prose column: a native
  collapsible `<details>` (no JavaScript) that CSS hides below a width breakpoint, the
  way the grouped sidebar stacks. Every heading gets a **server-generated** anchor `id`
  (heading text slugified, deterministically collision-disambiguated), so the rail's
  `#anchor` links resolve natively inside the sandbox. A page with fewer than 2 H2/H3
  headings, or a non-prose / full-document artifact, renders exactly as before (no empty
  rail). Heading text reaches the rail only server-side HTML-escaped — the CSP, sandbox,
  and Trusted-Types boundary are untouched.

### Fixed

- Security suite: the Gap-2 "markdown rendered to HTML" probe is now attribute-tolerant,
  matching the intentional heading `id` from the new TOC rail (no change to any sandbox /
  CSP / isolation assertion).

## [0.9.0] - 2026-08-14

Adds an **opt-in, LAN-reachable loopback serve** so another device on your local
network can view a served space — without weakening the artifact security model.
The DNS-rebinding protection is preserved as an allowlist, not dropped; the default
behaviour is byte-compatible loopback-only. The artifact sandbox/CSP/airlock are
unchanged (`./test-security.sh` 48 checks + Wave 2a green, plus new LAN-serve probes).

### Added

- **`glasspad loopback serve --bind <LAN-IP>`** — opt in to serving a space on the
  local network, reachable from other LAN devices. Off by default (no flag →
  byte-compatible loopback-only). The bind address must be a **literal private
  IPv4** (RFC 1918); a wildcard `0.0.0.0` and public addresses are refused. Also
  configurable via `.glasspad.yaml` / home config per the existing per-key merge.
  A loud startup warning names the exact reachable URL and notes the server carries
  no API key — a trusted-LAN convenience, never a public bind.

### Security

- **DNS-rebinding protection preserved under LAN mode.** The Host-header guard is
  extended to an **allowlist** (loopback hosts + the one explicitly-configured bind
  host), not disabled: a foreign `Host` sent to the LAN socket is still refused
  (`421`). The artifact sandbox, CSP, egress (`connect-src 'none'`), and the
  return-channel airlock are unchanged — the LAN origin is only *added* to the host
  set. Hardened per a 4-model review (literal-private-IPv4-only, home-directory-only
  bind, authority guard) with 13 new adversarial LAN probes.

## [0.8.0] - 2026-08-13

Makes a structured docsite a first-class glasspad space: **grouped/nested navigation
and a generated landing index**, so a docsite like aggountant's `design-v2` shape
(grouped spec/ADRs/stints + companion docs) ports onto glasspad driven only by a
manifest and slug-safe markdown — no bespoke index/sidebar generator. Everything stays
**structure-only** (no glasspad-owned content). The artifact security contract is
unchanged: every page stays a null-origin sandboxed iframe (`./test-security.sh` 48
checks + Wave 2a green).

### Added

- **Grouped, one-level-nestable nav** via an optional manifest `groups:` list. Each
  group has a `label` and ordered `members`; a member is a bare slug or a map with
  `title`/`desc`/`children` (one level of companion nesting). `Space.nav` stays the
  complete slug allowlist — groups are display curation only, so ungrouped pages stay
  reachable. No `groups:` → byte-compatible flat lexicographic nav (backward compatible,
  `#[serde(default)]`).
- **Generated grouped landing/index.** A space with no `index`/`home` page (and either
  declared groups or ≥2 pages) synthesizes a grouped landing artifact — docs listed by
  group, each with a description (manifest `desc:` → doc's first paragraph → none) —
  replacing the old redirect stub. It flows through `serve`, static `build` (emitted as
  `index.html`, no redirect), and hosted publish, and is idempotent. A single ungrouped
  page keeps the old redirect behavior.
- **Manifest-level companion mapping.** A group member's `children:` pairs slug-safe
  companion pages (e.g. `backtest` + `backtest-arkkitehdille`) under their parent in the
  nav, one level deep. glasspad stays slug-strict — dotted stems (`x.arkkitehdille.md`)
  remain the producer's preprocessor concern (companion *discovery* stays out of scope).

### Fixed

- **Critical (pre-existing): iframe `title`-attribute sandbox-escape.** A duplicate
  attribute could smuggle an `allow-same-origin` grant into an artifact iframe's
  `sandbox`, breaking the null-origin guarantee. Now fixed with a regression test.

## [0.7.0] - 2026-08-12

Reshapes the CLI around a single default: **hand glasspad markdown, get a URL.**
`publish` is now THE verb, and where a page lands is decided by config, not by the
agent picking a subcommand. This is a **breaking** surface change — several loopback
verbs are removed with no back-compat aliases. The artifact security contract is
unchanged: every page, hosted or loopback, stays a null-origin sandboxed iframe with
`connect-src 'none'` (`./test-security.sh` 48 checks + Wave 2a green).

### Added

- **`publish <path>` is the default verb.** A `.md`/`.markdown`/`.html` **file** is a
  one-page space; a **directory** is an N-page space. Markdown is rendered automatically
  via the space model. One verb, markdown-first — `publish` and `publish-space` are
  unified.
- **Config-driven target (`loopback | hosted`).** A new per-key config merge resolves
  each key independently through repo-root **`.glasspad.yaml`** → `~/.config/glasspad/config.yaml`
  → built-in default (`target: loopback`), so zero-config still just serves loopback.
  Keys: `target`, `server`, `api_key`, `template`, `space_key`, `favicon`. `target: hosted`
  uploads the space and returns the `/p/<slug>/…` URL (idempotent via `space_key`);
  `target: loopback` spawns/reuses a live-reload server and opens the local URL.
- **`api_key` indirection.** The `api_key` config key accepts an env-var or key-file
  reference, not only an inline secret — room for a future multi-worker credential model
  without a schema break.
- **Emoji SVG favicon.** Published and built pages carry a zero-dependency inline SVG
  emoji favicon on the outer served/built document, sourced from `.glasspad.yaml`
  (`favicon: 🚀`) with a default fallback. The emoji is strictly validated and XML-escaped;
  the artifact sandbox is byte-for-byte unaffected.

### Changed

- **Loopback management is now advanced**, regrouped under **`glasspad loopback <serve|open|stop>`**
  (help-only; not the standard flow). `glasspad build <space> <out>` is retained as an
  advanced static-output/debug verb.
- **`src/skill.md` rewritten** around the publish-first default; the old "default to
  loopback serve" mode table is gone.

### Removed

- **`serve`, `create`, `render`, `open`, and top-level `stop`** are removed as top-level
  verbs (no back-compat aliases). Their behavior is absorbed into `publish` and the
  `glasspad loopback` group.

## [0.6.0] - 2026-08-11

Turns glasspad into a **multi-page hosted docsite** tool: publish a whole directory
of linked artifacts as one hosted space with working in-space nav + relative links,
and hand it **markdown** directly instead of pre-rendered HTML. The artifact sandbox is
unchanged — every page, including one rendered from markdown, stays a null-origin
sandboxed iframe with `connect-src 'none'`.

### Added

- **Multi-page hosted publish (space ingest).** `glasspad publish-space <dir>`
  ingests a directory of linked `.html` artifacts into one hosted namespace
  (`/{space}/…`) on a `host-serve` instance, with the in-space bridge nav and
  cross-page relative links resolving across pages — the local `serve` experience on
  the hosted server. A stable space slug makes re-publish update in place (idempotent);
  cross-space/cross-tenant access is refused with an opaque `404`.
- **Markdown-native spaces.** `serve` / `build` / `publish-space` now treat a
  directory of `.md` as a space — each `.md` is rendered through the template seam into
  an artifact (slug = filename stem) with nav + relative links working, so a producer
  can hand glasspad the markdown directory directly. Existing `.html`-only spaces render
  identically (additive); markdown that embeds hostile HTML cannot escape the sandbox
  (covered by new security probes).

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
