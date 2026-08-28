---
created: 2026-08-28
updated: 2026-08-28
type: feature
reporter: jari
status: untriaged
priority: normal
provenance: agent:aggountant-wrapup
source_ref: agent:aggountant-wrapup/reporter:jari/id:aggountant-2026-08-28-glasspad-stable-republish
---

# Prevent accidental duplicate spaces when republishing a source path

## Description

Prevent accidental duplicate spaces when republishing a source path

Republishing the same standalone Markdown source with the same command created a second hosted space instead of updating the first one.

First command:

    glasspad publish issues/chart-vat-encoded-accounts/decision-brief.md

returned space `gfoasfiifl76dyt2hyccu2re2m`. After editing the same file, repeating that exact command returned a new space `n7iahq2rtjfyj4rs7o5ttrabci`. Updating the intended page required knowing and passing:

    glasspad publish issues/chart-vat-encoded-accounts/decision-brief.md \
      --update gfoasfiifl76dyt2hyccu2re2m

This easily leaves an unintended duplicate page and silently changes a URL already given to a user.

Expected: provide a safe stable-republish path keyed by the source identity, or detect that the same source was published before and warn/refuse unless the caller explicitly requests a new space. The normal repeat command should not silently create an accidental duplicate.
