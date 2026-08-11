---
created: 2026-08-11
updated: 2026-08-11
type: feature
status: open
priority: normal
blocked_by: ['@multipage-hosted-space']
---

# Markdown-native spaces (serve/build/space-publish render a dir of .md)

## Description

Gap 2 of the tilictl-docsite use case (split from multipage-hosted-space, which is Gap 1).

Today serve/build scan .html artifacts ONLY — a directory of .md renders 0 pages. Only single-file 'render <x.md> --template' handles markdown.

Ask: let serve / build / and the hosted space-publish (Gap 1 = multipage-hosted-space) treat a directory of .md as a space — render each .md through a template into an artifact (slug = filename stem), so producers can just hand it the markdown. Per-space nav already exists via glasspad.yaml. Reuse the existing single-file md render path (src/artifact_host/render.rs).

BLOCKED BY multipage-hosted-space (Gap 1) — builds on the space-ingest/namespace plumbing and shares Lane B hot files (src/hosted/*, src/cli.rs, src/main.rs). Do NOT start until Gap 1 lands.

Touches production code — run /llm-review (+ /assess-findings) before merging. Keep the artifact sandbox/security contract intact; ./test-security.sh must stay green.
