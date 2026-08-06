---
created: 2026-08-06
updated: 2026-08-06
type: feature
status: open
priority: high
related: ['@markdown-template-render']
blocked_by: ['@markdown-template-render']
---

# Hosted share-server run mode (public read, multi-agent ingest)

## Description

A second run mode beside the loopback live server: a long-lived **hosted share
server** + a `glasspad publish` client. The loopback `serve` stays UNCHANGED
(keeps its DNS-rebinding Host guard); the public bind + auth live only here.

- **Ingest:** API-key-authenticated push from many agents/machines (bearer token
  in local config). This is the write surface glasspad lacks today.
- **Read:** capability-slug public URLs (`/p/<slug>`, noindex), no read auth
  ("hold the link"; accounts are a later feature). Artifact bodies still served
  null-origin sandboxed — which is *why* public read is safe.
- **Storage:** immutable pages, retention/GC (~90 days), multi-tenant spaces.
- Generic/infra-agnostic (`--public-host`, `--api-key-file`, template dir,
  retention) — a specific deployment is the operator's config, not crate code.

Mirrors publish-html's proven client/server shape. Depends: markdown-template-render.
Part of the agent→browser-HTML consolidation. Full design + rationale: homebase `issues/glasspad-html-consolidation/design.md` (Option D). These features make glasspad the single canonical agent→HTML surface.
Ref: src/server.rs (keep loopback path intact), src/artifact_host/.
