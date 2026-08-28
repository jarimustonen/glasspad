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

## Triage analysis

**Verdict: expected behaviour, with ambiguous wording in the authoring guide. Severity: low.** No attachments directory exists for this issue.

A hosted publish returns `/p/<space>/`, which is deliberately the trusted space shell. That route always renders Glasspad's header/navigation and a null-origin sandboxed iframe whose source is `/p/<space>/_c/<artifact>`. Full-document detection applies only at that content route: `wrap::render_artifact` returns a document beginning with `<!doctype html>` or `<html>` byte-for-byte instead of adding Glasspad's fragment skeleton, CSS, or bridge. It does not replace the outer shell. Targeted tests for full-document passthrough and hosted shell/content separation both pass.

The public contract does not promise a chrome-free top-level hosted presentation. README explicitly says every artifact opens in a sandboxed iframe, and the original host plan distinguishes the space-entry shell from the raw `_c` artifact document. The bundled skill's “served verbatim; you own the whole page” wording can nevertheless reasonably be read as describing the whole browser tab rather than the iframe document, so it should be clarified.

Affected users are those using hosted publishing for a standalone presentation or pixel-exact/raw preview. Their authored bytes are preserved, but Glasspad chrome consumes viewport space and the authored document cannot control the top-level page, making that use case unsuitable; ordinary one-page and multi-page Glasspad spaces remain correct. Documentation should say “verbatim inside the artifact iframe” and distinguish artifact ownership from the trusted shell. If standalone hosted presentation is a desired product use case, separately design an explicit chrome-free route/mode without weakening the sandbox/CSP contract; no application-code fix is implied by this triage.
