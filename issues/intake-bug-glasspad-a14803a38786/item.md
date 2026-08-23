---
created: 2026-08-23
updated: 2026-08-23
type: bug
reporter: jari
status: untriaged
priority: normal
provenance: agent:homebase-wrapup
source_ref: agent:homebase-wrapup/reporter:jari/id:homebase-wrapup-glasspad-full-document-chrome-20260823
---

# Hosted full-document HTML shows Glasspad chrome

## Description

Hosted full-document HTML shows Glasspad chrome

## Observed

Publishing a complete HTML document from the tw host with:

```sh
glasspad publish --no-open /tmp/tw-client-browser-test.html
```

returned a hosted URL successfully, but opening that URL showed Glasspad headers/chrome around the page. The input began with `<!doctype html>` and contained a complete `<html>` document.

This made the hosted result unsuitable for a standalone presentation or raw preview where the authored document must be the entire visible page.

## Expected

The documented full-document mode says complete HTML is served verbatim. A full document should therefore render without Glasspad headers/chrome, or Glasspad should provide and clearly document an explicit chrome-free/raw hosted mode if the outer shell is architecturally required.

## Context

The immediate private-preview need was solved separately with `tw browser-open`, which transfers the file byte-for-byte to the attached client. This report concerns the mismatch between Glasspad's full-document contract and its hosted presentation, not a request to add tw transport to Glasspad.
