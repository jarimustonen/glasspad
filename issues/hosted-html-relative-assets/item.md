---
created: 2026-09-24
updated: 2026-09-24
type: bug
reporter: jari
status: fixed
priority: high
lane: hosted-asset-routes
closed: 2026-09-24
closed_by: agent
---

# Hosted HTML relative assets resolve to missing content route

## Description

A hosted HTML space containing `index.html` and sibling `assets/ready.avif` renders broken images when the HTML contains `<img src="assets/ready.avif">`. Reproduced on https://glasspad.maalla.dev/p/54fzloidunxutt3dk2un7xt55i/ (public capability URL; keep this slug out of broad logs). The shell and `_c/index` return 200, but the browser resolves `assets/ready.avif` against `/p/<space>/_c/index` to `/p/<space>/_c/assets/ready.avif` (404), while `/p/<space>/assets/ready.avif` returns 200. All six AVIFs on this page fail. Do not require authors to use `../assets/`, a hosted URL implementation detail. The 0.18.1 fix covered relative Markdown assets, not this HTML case.

## Reproduction

1. Publish an HTML directory with `index.html` referring to `<img src="assets/ready.avif">` and an `assets/ready.avif` sibling directory.
2. Open the hosted space root, observe a broken image; inspect the browser resource URL and the successful canonical asset URL.

## Acceptance Criteria

- [x] HTML relative links to sibling assets work as authored for both a root index and nested HTML pages; preserve relative navigation and fragments.
- [x] Browser tests cover images, CSS, JS and nested paths in the null-origin iframe under the existing public-host/CSP restrictions; no cross-space traversal or exposure is introduced.
- [x] The affected hosted page (or an equivalent fixture) shows all images after the fix without altering its source to `../assets`.
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo publish --dry-run`, and `./test-security.sh` pass before release.

## Tests Run

- `cargo fmt --all --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed.
- `cargo test` — passed, including persisted hosted-space regression.
- `cargo publish --dry-run` — passed (warned that version 0.18.2 already exists and a locked transitive dependency is yanked; no upload).
- `./test-security.sh` — passed (51 browser checks and final Wave 2a space-model probes); exercised loopback and hosted HTML images/CSS/JS, relative navigation/fragments and traversal/cross-space denial.
- `/llm-review` + `/assess-findings` — three test/fixture/docs findings fixed; independent security verifier found no remaining actionable issue. Optional model-performance logging failed due to `haapa` SSH authentication; review and tests unaffected.

## Implementation Notes

`/{space}/_c/assets/{*path}` now aliases the existing `space_asset` handler, so full HTML documents resolve authored `assets/...` from the iframe URL to the same per-space scanned asset map as the canonical route. Both new and persisted publications work without altering stored HTML or the private published example. The alias retains MIME, sandbox CSP, nosniff, no-store, public-host and hosted noindex policy, and does not accept unscanned paths or cross-space reads. A root HTML page and a second HTML page (`guide`) exercise AVIF, CSS, JS, anchors and same-space shell navigation. The first security-suite attempt failed because the new loopback alias probe ran after the suite had shut down that loopback server; the probe was moved before shutdown and the complete suite passed on rerun. The initial dry-run attempt refused an uncommitted tree; it passed after the implementation commit.
