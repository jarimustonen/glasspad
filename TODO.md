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

**Waves 1, 2, and Wave 3 batch 1 are DONE (2026-07-24).** Standing autonomy
granted (`AGENTS.md`). On `master`: security gate, space model + live serving,
base libs (`base.css`/`charts.js`/`bridge.js`/`manifest.json`), legacy v0.1
surface removed (D2). Suite green: 31 browser checks + Wave 2a probes + vega/eval;
all cargo tests pass.
**Next job: Wave 3 batch 2 — 3a CLI** (`serve`/`create`/`open`, fragment-vs-full-
doc detection), then **Wave 4** (nav chrome) → **Wave 5** (final removals:
`src/spec/*`, `dashboard.js`, data parsers → `glasspad data`). `structured-api-
errors` is moot now `/api/pads` is gone; the CLI error contract follows
`AGENTS-AI-FIRST-CLI.md`.

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
- [x] **Phase 1 — Security contract + iframe shell.** DONE (2026-07-24,
  commits `01c1498`/`4969c2d`/`4823d53`). Null-origin sandbox host + full
  CSP/egress contract + control-plane guards, alongside the untouched v0.1 path.
  `test-security.sh` = 21-check adversarial headless-Chromium suite (all pass) +
  55 Rust contract/guard tests. Vega-Lite resolved: `'unsafe-eval'` required and
  frozen into `script-src` (recorded `design.md §4`). `/llm-review` (Gemini 3.1
  Pro, GPT-5.6-sol, Opus 4.7, DeepSeek v4 Pro) + `/assess-findings`: 9 FIX
  applied, 3 design forks **RESOLVED by PO (2026-07-24)** — see `design.md §10`:
  1. `allow-top-navigation-by-user-activation` → **KEEP** (needed Wave 3b full-doc
     nav); revisit at Wave 3b.
  2. Legacy `/{id}` pad renderer + `/api/pads` → **REMOVE (unused)** — delete as
     dead code. Sequenced **after Wave 2** (shares the router/`main.rs` hot file
     with 2a); folded into / pulled ahead of Wave 5 removals.
  3. Artifact CSP names whole loopback host → **DEFER to Wave 2a** (weigh a
     separate asset origin).
- [x] **Phase 2 — Space model + live directory serving.** DONE (2026-07-24,
  `af7f614`/`25b4d5a`/`311575d`). Directory scanner (artifacts + `assets/`),
  atomic all-or-nothing Snapshot swap (torn-read-free), slug grammar +
  reserved-name/collision hard errors, symlink rejection + canonical containment,
  MIME allowlist + nosniff + `CSP:sandbox`, no wildcard CORS, per-file/space/
  entry/manifest limits, comment/quote-aware title tokenizer (inserted as text),
  dependency-free 500ms fingerprint-poll watcher → SSE reload. New traversal/
  symlink/hostile-asset probes in `test-security.sh`; 21 Wave 1 checks stay green.
- [ ] **Phase 3 — CLI** (Wave 3 batch 2, in flight). `serve ./dir`,
  `create ./file.html`, `open`; fragment vs full-document detection
  (BOM/whitespace/comment tolerant). AI-first: `--json`, strict validation,
  no interactive prompts, informative errors.
- [x] **Phase 4 — Base libraries** under `/_gp/v1/`. **4-static DONE**
  (`6636493`/`85fe95d`): `base.css` (`--gp-*`, light/dark/auto), `charts.js`
  (`gp.chart` over vendored vega/vega-lite/vega-embed), `manifest.json`. **4-bridge
  DONE** (Wave 3b, `18a4d39`): `bridge.js` — same-space relative-link nav via the
  validated postMessage bridge + theme toggle, auto-injected into fragment
  artifacts (in `<head>` before untrusted bytes); Wave 1 bridge validation
  strengthened (exact `{type,slug}` schema); frozen CSP untouched. Suite → 31
  browser checks.
- [x] **D2 — legacy-route removal DONE** (Wave 3, `bf2fbef`): removed `/{id}` pad
  renderer + `/api/pads` CRUD + exclusively-dead code (`PadStore`, `renderer.rs`,
  `security/csp.rs`, `Pad`/`PadMeta`, `uuid` dep); binary reaches shared
  spec/data/security via the lib crate. DSL/parsers/client-renderer left for Wave 5.
  Binary clippy warnings dropped 33→5. Added `no_mutating_same_origin_surface_exists`
  invariant test.
- [ ] **Phase 5 — Nav + cross-links.** Parent-frame nav chrome; bridge intercepts
  same-space relative links.
- [ ] **Phase 6 — Removals + migration.** Coupling audit of the "reused" server
  pieces first; then delete `src/spec/*`, `src/client/dashboard.js`; **also delete
  the legacy `/{id}` pad renderer + `/api/pads` handlers (PO confirmed unused —
  decision D2)**; move data parsers to an optional `glasspad data` helper. **Only
  after Phase 1 tests pass on the new path.** Update `skill.md`, `README.md`,
  `DESIGN.md` pointers. NB: the legacy-route removal can be pulled ahead of the
  full Wave 5 once Wave 2 lands (it only needs the router/`main.rs` free of the
  parallel Wave 2a edit) — sequence it, never parallel with a router-touching wave.

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
