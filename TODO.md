# TODO — Glasspad v0.2 handoff

Orchestrator entry point for `/stint`. This is the round-by-round plan; the
authoritative detail lives in the issue tracker (`issuectl`), not here.

## Where we are

Glasspad is being **rewritten (v0.2)**: the calling agent authors **HTML
directly** and Glasspad hosts it in a sandboxed iframe. This deletes the ~6000-
line section-DSL (`src/spec/*`, `src/client/dashboard.js`) and the data parsers.

- **Scope: localhost-only.** No team/cloud, accounts, or persistence backends.
- Everything is driven by one feature issue: **`html-artifact-host-rewrite`**
  (`issuectl show html-artifact-host-rewrite`). Read its `plan.md` and
  `design.md` before touching code — the security model must be built first.
- Design decisions are **locked** (see the issue's `## Decisions` + `analysis.md`):
  D1 null-origin sandbox + `CSP: sandbox` response header + egress CSP naming the
  host; D2 first-class assets/data; D3 serve the directory live, no store; relative
  links (no `glasspad:` scheme).

## ▶ Start here (next session)

**Phase 0 is done** — the wave plan exists at
`issues/html-artifact-host-rewrite/wave-plan.md`. Read it first; it is the
execution schedule (5 waves), and `plan.md` + `design.md` are the *what* and the
security model it implements.

**Next job: Wave 1 — Phase 1 (security contract + iframe shell).** Spawn it as
an **interactive** worktree (`/worktree-code`), foreground, human-reviewed — it
is the security gate for the whole rewrite, no auto-merge, merged by the user via
`/worktree-merge`. Its definition-of-done (adversarial browser test suite,
Vega-Lite `'unsafe-eval'` question, clean build/clippy/browser) is in the wave
plan under "Wave 1". Everything else branches from Wave 1's landed result.

**Merge policy (per wave).** See the wave plan's merge-policy table. Summary:
Wave 1 interactive/human-gated; Waves 2–5 autonomous (`/worktree-spinoff
--headless`, self-merging) with `/llm-review` in the brief for every unit that
touches production/security code. Confirm the "revisit gate" (units disjoint on
the shared axum router) before running any parallel wave.

## Round plan (one worktree per phase, in order)

Phases are ordered for safety: the security contract and its adversarial tests
land **before** any old code is deleted. Each phase = one worktree unit tracked
under `html-artifact-host-rewrite` (Phase 1 interactive; later phases per the
wave plan's merge policy).

- [x] **Phase 0 — Wave plan.** `wave-plan.md` written (2026-07-23). Waves: 1=Phase 1
  (interactive gate); 2a∥2b=Phase 2 ∥ Phase 4-static; 3a∥3b=Phase 3 CLI ∥
  Phase 4-bridge; 4=Phase 5 nav; 5=Phase 6 removals. See the issue's `wave-plan.md`.
- [ ] **Phase 1 — Security contract + iframe shell.** URL topology
  (`/{space}/{slug}` shell, `/{space}/_c/{slug}` raw content, `/_gp/v1/*`);
  artifact-response headers (`CSP: sandbox allow-scripts` + egress CSP naming the
  host + `nosniff` + `Permissions-Policy` deny-list); null-origin iframe;
  validated `postMessage` bridge (`event.source` check). **Ship the adversarial
  browser test** (per-channel exfil, sandbox-escape, direct-open, postMessage
  abuse). Verify what Vega-Lite needs (`'unsafe-eval'`?).
- [ ] **Phase 2 — Space model + live directory serving.** Snapshot scanner,
  slug grammar + collision/reserved-name rejection, asset routing + MIME + size
  limits, atomic rescan, filesystem watch + SSE reload.
- [ ] **Phase 3 — CLI.** `serve ./dir`, `create ./file.html`, `open`; fragment
  vs full-document detection (BOM/whitespace/comment tolerant).
- [ ] **Phase 4 — Base libraries** under `/_gp/v1/`: `base.css` (existing design
  system), `charts.js` (`gp.chart`), `bridge.js` (auto-injected), `manifest.json`.
- [ ] **Phase 5 — Nav + cross-links.** Parent-frame nav chrome; bridge intercepts
  same-space relative links.
- [ ] **Phase 6 — Removals + migration.** Coupling audit of the "reused" server
  pieces first; then delete `src/spec/*`, `src/client/dashboard.js`; move data
  parsers to an optional `glasspad data` helper. **Only after Phase 1 tests pass
  on the new path.** Update `skill.md`, `README.md`, `DESIGN.md` pointers.

## Backlog / parallel decisions (not blocking the rewrite)

- `structured-api-errors` (decision) — resolve before/with Phase 3's CLI surface.
- `auth-status-codes` (decision) — minor under localhost-only; may be closed.
- `mcp-integration` (epic) — after the core host works; how agents reach glasspad.

## Verify / deploy (localhost)

Per `CLAUDE.md`: after editing client assets, `cargo build`, restart server,
create a new pad. Use `./test-browser.sh` (check `./test-browser.sh errors`
first) for browser automation. Phase 1's adversarial tests are the gate for the
whole rewrite.

## Notes for the orchestrator

- Keep `main` clean — commit issue/status changes immediately (`CLAUDE.md`).
- 43 v0.1 section-DSL issues were closed `obsolete`; don't resurrect them.
- Track all planning under the issue, not as loose files.
