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
