---
created: 2026-09-24
updated: 2026-09-24
type: bug
reporter: jari
status: open
priority: high
lane: hosted-asset-routes
---

# Hosted HTML relative assets resolve to missing content route

## Description

A hosted HTML space containing `index.html` and sibling `assets/ready.avif` renders broken images when the HTML contains `<img src="assets/ready.avif">`. Reproduced on https://glasspad.maalla.dev/p/54fzloidunxutt3dk2un7xt55i/ (public capability URL; keep this slug out of broad logs). The shell and `_c/index` return 200, but the browser resolves `assets/ready.avif` against `/p/<space>/_c/index` to `/p/<space>/_c/assets/ready.avif` (404), while `/p/<space>/assets/ready.avif` returns 200. All six AVIFs on this page fail. Do not require authors to use `../assets/`, a hosted URL implementation detail. The 0.18.1 fix covered relative Markdown assets, not this HTML case.

## Reproduction

1. Publish an HTML directory with `index.html` referring to `<img src="assets/ready.avif">` and an `assets/ready.avif` sibling directory.
2. Open the hosted space root, observe a broken image; inspect the browser resource URL and the successful canonical asset URL.

## Acceptance Criteria

- [ ] HTML relative links to sibling assets work as authored for both a root index and nested HTML pages; preserve relative navigation and fragments.
- [ ] Browser tests cover images, CSS, JS and nested paths in the null-origin iframe under the existing public-host/CSP restrictions; no cross-space traversal or exposure is introduced.
- [ ] The affected hosted page (or an equivalent fixture) shows all images after the fix without altering its source to `../assets`.
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo publish --dry-run`, and `./test-security.sh` pass before release.

## Tests Run

## Implementation Notes
