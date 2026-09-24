---
created: 2026-09-24
updated: 2026-09-24
type: improvement
status: open
priority: normal
lane: shell-chrome
collision: [crates/glasspad-core/src/artifact_host/shell.rs, crates/glasspad-cli/src/artifact_host/assets/base.css]
---

# Move viewer theme control into sidebar and remove redundant header

## Description

In a single-page prose space (observed in the real hosted native-agent-host visual-acceptance test), the legacy trusted viewer chrome still paints a horizontal bar above the designer's prose page: a redundant truncated page-title tab and a top-right `Theme: Auto` button. This bar is not part of the approved page design. Jari requests removing the ugly header and putting a well-integrated theme selector in the sidebar. The prose TOC rail is inside a null-origin sandboxed iframe while the current theme button is in the trusted shell; do not move privileged shell controls into untrusted artifact code without a justified secure boundary design.

## Acceptance criteria

- [ ] No redundant header strip or truncated single-page title chip is painted above a prose artifact. Page content starts without wasted top chrome.
- [ ] A usable, visually coherent theme selector appears in the sidebar context (preferably next to the prose TOC on desktop; on phone still discoverable), supports auto/light/dark, keyboard and accessible naming/focus.
- [ ] Multi-page spaces retain usable same-space navigation (flat and grouped), active-page indication, and theme choice; no unreviewed removal of page navigation or submission status.
- [ ] Theme persists and updates the trusted shell and sandboxed artifact without a flash on navigation; iframe remains null-origin and shell CSP/Trusted Types/postMessage allowlists remain intact.
- [ ] Real hosted or equivalent route browser regression at desktop and phone widths, both single and multi-page; no overflowing/overlapping controls; screen/keyboard checks.
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo publish --dry-run` and the entire `./test-security.sh` (51 + Wave 2a) remain green.
