---
created: 2026-08-28
updated: 2026-09-02
type: bug
reporter: jari
status: fixed
priority: normal
provenance: chat
lane: template-header
commits:
- hash: 454bc9d
  summary: align page chrome with artifact theme
- hash: 059017c
  summary: make shell theme transitions paint-safe
- hash: da88210
  summary: align page chrome with artifact theme (rebased)
- hash: 47c6f62
  summary: make shell theme transitions paint-safe (rebased)
closed: 2026-09-02
---

# Base template header ignores theme and duplicates title

## Description

Perustempaatin hraeder palkki ei tunnusta päivä / yö vaihtelua ja sitten siinä on itse asiassa glasspad artikkelin / tekstin otsikko kahteen kertaan. Tästä voisi tehdä uuden issuen ja lanettaa sen

## Acceptance Criteria

- [x] Header chrome follows live light, dark, and automatic day/night theme changes without reload.
- [x] The built-in page chrome no longer renders a separate current-artifact title above the artifact content.
- [x] Browser regressions preserve the artifact H1, document title, and iframe accessible title.
- [x] Format, clippy, test, and complete security gates pass.
