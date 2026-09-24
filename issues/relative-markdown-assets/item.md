---
created: 2026-09-23
updated: 2026-09-24
type: bug
reporter: jari
status: fixed
priority: normal
lane: artifact-assets
collision: [crates/glasspad-cli/src/artifact_host/space.rs, crates/glasspad-cli/src/hosted/mod.rs]
closed: 2026-09-24
closed_by: pi
---

# Relative Markdown assets break in hosted artifacts

## Description

Hosted Markdown pages render relative asset URLs against the private content route `/_c/<artifact>` instead of the documented space asset root. An inline Markdown image such as `![Screenshot](./assets/screenshot.avif)` becomes `<img src="./assets/screenshot.avif">`; the browser therefore requests `/p/<space>/_c/assets/screenshot.avif`, which is denied, although the uploaded asset is available at `/p/<space>/assets/screenshot.avif`.

This contradicts the documented space model: `assets/*` is served by path and Markdown/HTML pages are expected to use ordinary relative links. It prevents screenshot-heavy Markdown acceptance reports from rendering inline in hosted spaces.

Observed with Glasspad 0.17.5 (`f022e3708d49`) at a hosted `https://glasspad.maalla.dev` deployment on 2026-09-23.

## Reproduction

Create a space:

```text
space/
  visual-acceptance.md
  assets/screenshot.avif
```

with:

```markdown
# Visual acceptance

![Screenshot](./assets/screenshot.avif)
```

Publish it hosted:

```sh
glasspad publish ./space --json
```

Fetch the artifact content and the two candidate image paths:

```sh
curl -fsSL 'https://HOST/p/SPACE/_c/visual-acceptance'
# contains: <img src="./assets/screenshot.avif" ...>

curl -I 'https://HOST/p/SPACE/_c/assets/screenshot.avif'
# 403

curl -I 'https://HOST/p/SPACE/assets/screenshot.avif'
# 200 image/avif
```

The trusted shell at `/p/<space>/visual-acceptance` embeds `/_c/visual-acceptance` in the null-origin iframe, so browser URL resolution produces the failing `/_c/assets/...` request.

## Expected behavior

Markdown-authored relative image/asset references resolve to the corresponding asset within the same space in both loopback and hosted targets, without requiring the producer to know the hosted capability slug or mount prefix.

A fix must preserve the null-origin sandbox, CSP and same-space navigation contract. Likely approaches are renderer-aware rewriting of local asset URLs to `/p/<space>/assets/...` or an equivalent safe routing/base mechanism; do not weaken the path allowlist or expose arbitrary files.

## Acceptance Criteria

- [x] A Markdown page containing `![alt](./assets/image.avif)` renders the uploaded image inline in hosted mode (verified against a local hosted server).
- [x] The equivalent loopback publish still works.
- [x] Relative same-space page navigation retains its existing bridge behavior.
- [x] Traversal, encoded traversal, symlink and cross-space asset access remain denied.
- [x] Documentation and a deterministic hosted/loopback regression test cover Markdown image and ordinary asset references.

## Quick Test

Publish a fixture with one Markdown page and one AVIF under `assets/`. Assert the rendered artifact's image loads successfully in Playwright and the network request reaches the allowed space asset route, then run the full host/security gates (`cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `./test-security.sh`).
