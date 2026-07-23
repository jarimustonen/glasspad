---
created: 2026-04-09
updated: 2026-07-23
type: epic
owner: jari
status: obsolete
priority: normal
slug: email-support
closed: 2026-07-23
---

# Email support

## Goal

Full email viewing support in Glasspad — from raw email data to a polished, interactive email UI. Covers message list rendering, detail view with HTML body, attachments, threading, and filtering.

## Issues

- **@rows-compact-visual-testing** Rows and compact layout visual testing (open)
- **@detail-view-filter-update** Detail view update on filter change (open)
- **@attachment-display** Attachment display component (open)

## Phases

### Phase 1: Core rendering
- [x] List section with detail view (@rows-compact-visual-testing, @detail-view-filter-update)
- [x] HTML body rendering in detail view
- [ ] Rows and compact layout visual testing (@rows-compact-visual-testing)

### Phase 2: Attachments & polish
- [ ] Attachment display component (@attachment-display)
- [ ] Detail view filter interaction (@detail-view-filter-update)
- [ ] Inline image handling (cid: references)

### Phase 3: Advanced features
- [ ] Email threading / conversation view
- [ ] Search within email body
- [ ] Sender avatars / contact info display

## Comments

Existing list section type already supports email-like data with columns, detail view, and HTML body rendering. This epic consolidates that work and tracks remaining polish.
