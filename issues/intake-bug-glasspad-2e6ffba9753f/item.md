---
created: 2026-10-15
updated: 2026-10-15
type: bug
reporter: jari
status: untriaged
priority: normal
provenance: agent:homebase-wrapup
source_ref: agent:homebase-wrapup/reporter:jari/id:open-habitat-wrapup-2026-10-15-glasspad-xdg
---

# glasspad publish --target hosted tries xdg-open on a headless server

## Description

glasspad publish --target hosted tries xdg-open on a headless server

Command (on haapa, headless tw server): `glasspad publish --target hosted <file.html>`

Observed: the publish succeeds, but glasspad then tries to open the URL locally and prints
`/usr/bin/xdg-open: 1032: lynx: not found`, `w3m: not found` and
`xdg-open: no method available for opening 'https://glasspad.maalla.dev/p/<slug>/'`.

Expected: on a host without a display (no DISPLAY/WAYLAND_DISPLAY, or a tw server role), do not attempt a local open; print the URL only, or route through `tw browser-open` when a seat is attached.

<!-- intakectl:analysis:start job:3992f19f-a03f-4bc1-9a1d-008bb8313084 generation:0 -->
## Triage analysis

- **Verdict:** Confirmed.
- **Severity:** Low; publish succeeds and prints the URL, but an unnecessary local opener emits noisy failure output on a headless host.
- **Affected area:** Hosted `publish` browser-opening behavior in `crates/glasspad-cli/src/cli/publish.rs`.
- **Repro status:** The report provides a concrete command and observed `xdg-open` failure on the headless server. The source path confirms the cause: `post_space_bundle` invokes `launch_browser` whenever `--no-browser` is absent, and Linux `launch_browser` spawns `xdg-open` without checking whether a graphical session is available.
- **Fix sketch:** Keep explicit `--no-browser` behavior. Before auto-opening a hosted URL on Linux, detect whether a display/session is available (at minimum `DISPLAY` or `WAYLAND_DISPLAY`) and skip the local opener when headless; still print the URL and report `browser_launched: false` in JSON. Avoid treating opener spawn success alone as proof that it can display a browser.
<!-- intakectl:analysis:end job:3992f19f-a03f-4bc1-9a1d-008bb8313084 generation:0 -->
