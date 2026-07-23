---
created: 2026-07-22
updated: 2026-07-22
type: feature
reporter: jari
assignee: jari
status: open
priority: high
slug: ridiculously-ambiguous-business
---

# Rewrite Glasspad as an HTML-artifact host

_Source: whole project — replaces the section-DSL architecture_

## Description

Rewrite Glasspad so the **calling agent authors HTML directly** and Glasspad
just hosts and serves it. Today the agent must encode content (charts, tables,
stats, pivots) into a rigid section-DSL expressed in YAML, validated by
~2000 lines of Rust and rendered by a ~3000-line client renderer. The goal is
to make this a **lightweight way for an agent to show HTML content to a user**,
with no content-DSL at all.

Requirements from the product owner:

- Agent defines content as **HTML**, not a structured YAML content schema.
- Support **multiple artifacts** per unit, with **links between them**.
- Provide **navigation chrome** and some sensible **base structures / base
  libraries** (design tokens, chart helper, link/theme bridge) — opt-in.
- Make it **as easy as possible for the calling agent** (convention over
  configuration; a directory of `.html` files is a valid space).
- Support three deployment modes: **localhost**, **team shared server**,
  **glasspad.ai hosted cloud**.

## Key decisions (settled with PO)

- **Security model**: arbitrary agent HTML + JS is allowed, rendered inside a
  sandboxed `<iframe sandbox="allow-scripts">` (null origin). On team/cloud, a
  **separate content origin** (per-space subdomain) isolates spaces from the app
  and from each other. See `design.md`.
- **Model**: a **Space** = a set of artifacts sharing a URL namespace + nav; an
  **Artifact** = one HTML view addressed by a slug. Cross-links via a
  `glasspad:<slug>` scheme resolved by a parent-frame bridge.
- **Persistence**: the on-disk format (a directory of `.html` + optional
  `glasspad.yaml`) IS the wire format. localhost serves a directory live (no
  persistence); team/cloud persist so links survive. The directory is the
  portable, repo-committable source of truth.
- **Data ingestion** (csv/json/mbox): cut from core; keep the parser code as an
  optional `glasspad data` CLI helper (file → JSON to stdout). Reversible.
- Container is named **space** (was "pad").

## Scope

See `plan.md` for the phased implementation plan and `design.md` for the
iframe / origin-isolation security model.

## Out of scope (follow-up)

- Accounts / real auth for team & cloud modes (token model generalizes to it).
- Wildcard-DNS + TLS automation for per-space subdomains.
