---
created: 2026-11-27
updated: 2026-11-27
type: feature
reporter: jari
status: done
priority: normal
lane: unlaned
closed: 2026-11-27
commits:
- hash: 6ad2f4f
  summary: top bar replaces the sidebar viewer chrome
- hash: ebbcf4d
  summary: focus fallback and stricter next-link probe
---

# Top bar replaces the sidebar viewer chrome

## Description

Replace the multi-page viewer chrome introduced in 0.18.2 (left sidebar on desktop, bottom dock on phones) with a low top bar in the trusted shell, chosen by the maintainer from a prototype comparison of three directions (current sidebar, an on-demand page panel, top bar).

The top bar, about 44 px high, carries:
- the space name and a page switcher (current page ▾) listing the space's pages with their groups and the active page marked;
- previous / next page links;
- a three-way theme control (light / dark / system).

On phones the same bar stays; the theme control moves to the bottom of the page-switcher menu. Single-page spaces show no switcher or previous/next, only a theme control in a corner. The prose table of contents stays inside the sandboxed page as today.

## Acceptance criteria

- [x] Desktop and phone, light and dark: the bar renders without horizontal scroll; long page titles truncate in the bar and wrap in the menu.
- [x] Grouped and flat navigation, active page and theme choice keep working; same-space navigation and submission status are not lost (the guarantees listed in `viewer-chrome-sidebar`).
- [x] The change is confined to the trusted shell template and CSS; the null-origin sandbox, CSP and the page→shell message allowlist are unchanged.
- [x] Keyboard: the switcher opens, navigates and closes with the keyboard; focus is visible.
- [x] Shell tests updated; full security suite and browser checks green; CHANGELOG `[Unreleased]` entry.

## Reference

The prototype harness (top-bar variant) was built from the shipped shell CSS; it is kept privately by the maintainer and its relevant markup/CSS can be handed to the implementer in the brief.
