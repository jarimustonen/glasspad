---
created: 2026-04-09
updated: 2026-04-09
type: epic
owner: jari
status: open
priority: normal
---

# E01. Bidirectional actions

_Continues phase 9 from TODO roadmap_

## Goal

Enable two-way interaction between CLI agents and the browser UI. Users can trigger actions (buttons, row actions, done/cancel) in the dashboard, and the CLI agent receives structured completion data via polling.

## Issues

- **#04** Rows/compact layout visual testing (open)
- **#05** Detail view filter update (open)
- **#06** Attachment display component (open)

## Phases

### Phase 1: Backend completion flow
- [ ] 9.1 Completion endpoint: `POST /api/pads/:id/complete`
- [ ] 9.2 `GET /api/pads/:id/completion` (CLI polls)
- [ ] 9.7 `--wait` CLI flag (blocking, timeout)
- [ ] 9.8 Pad locks after completion (409)

### Phase 2: UI action buttons
- [ ] 9.3 Action buttons in detail view
- [ ] 9.4 `row_actions` in tables
- [ ] 9.5 Done button + Cancel button
- [ ] 9.6 Pending actions JS state

## Notes

Ref: `04-spec-contract.md` section 7, `08-arch-bidirectional-actions.md`

Design docs are in `history/` (gitignored). Key files:
- `history/04-spec-contract.md` — contract spec
- `history/08-arch-bidirectional-actions.md` — architecture
