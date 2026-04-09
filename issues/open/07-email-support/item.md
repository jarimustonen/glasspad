---
created: 2026-04-09
updated: 2026-04-09
type: epic
owner: jari
status: open
priority: normal
---

# E07. Email support

## Goal

Full email viewing support in Glasspad — from raw email data to a polished, interactive email UI. Covers message list rendering, detail view with HTML body, attachments, threading, and filtering.

## Issues

- **#04** Rows and compact layout visual testing (open)
- **#05** Detail view update on filter change (open)
- **#06** Attachment display component (open)

## Phases

### Phase 1: Core rendering
- [x] List section with detail view (#04, #05)
- [x] HTML body rendering in detail view
- [ ] Rows and compact layout visual testing (#04)

### Phase 2: Attachments & polish
- [ ] Attachment display component (#06)
- [ ] Detail view filter interaction (#05)
- [ ] Inline image handling (cid: references)

### Phase 3: Advanced features
- [ ] Email threading / conversation view
- [ ] Search within email body
- [ ] Sender avatars / contact info display

## Notes

Existing list section type already supports email-like data with columns, detail view, and HTML body rendering. This epic consolidates that work and tracks remaining polish.
