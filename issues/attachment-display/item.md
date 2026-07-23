---
created: 2026-04-09
updated: 2026-04-09
type: improvement
reporter: jari
assignee: jari
status: open
priority: normal
slug: utterly-fascinated-curve
---

# Attachment display component

_Source: list section detail view_
_Epic: **@email-support** Email support_

## Description

Attachments in list items (e.g. email attachments) are currently shown as a plain text field. They should be rendered as a dedicated component with file names, sizes, and types clearly visible.

Currently `cid:` images in HTML bodies are preserved as tags but don't render (inline attachments are not extracted). This issue covers displaying attachment metadata, not necessarily rendering inline content.
