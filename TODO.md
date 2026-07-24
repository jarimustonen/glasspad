# TODO — Glasspad v0.2 handoff

Orchestrator entry point for `/stint`. This is the round-by-round plan; the
authoritative detail lives in the issue tracker (`issuectl`), not here.

## Where we are

**The v0.2 rewrite is COMPLETE (2026-07-24).** Glasspad is now the lightweight
HTML-artifact host: the calling agent authors **HTML directly** in a directory,
`glasspad serve`s it, and it renders in a null-origin sandboxed iframe with nav
chrome + base libraries. The old ~6000-line section-DSL path is gone.

- **Scope: localhost-only.** No team/cloud, accounts, or persistence backends.
- Feature issue **`html-artifact-host-rewrite`** is **CLOSED (done)**. Its
  `plan.md` / `design.md` / `wave-plan.md` remain the record of the architecture
  and security model; `design.md §10` holds the resolved review decisions.
- Delivered across 5 waves / 27 commits: Wave 1 security gate (null-origin
  sandbox + CSP/egress contract + validated postMessage bridge), Wave 2a space
  model + live serving, Wave 2b base libs (`base.css`/`charts.js`/`manifest.json`),
  Wave 3a CLI (`serve`/`create`/`open`), Wave 3b `bridge.js`, D2 legacy-surface
  removal, Wave 4 trusted-parent nav chrome, Wave 5 section-DSL teardown +
  `glasspad data` helper. Net −5,500 lines.
- **Green baseline on `master`:** `./test-security.sh` = 41 browser checks +
  Wave 2a probes; `cargo build`/`clippy --all-targets`/`test` all clean.

## ▶ Start here (next session)

The rewrite is done — **no rewrite work remains.** The natural next target is the
**`mcp-integration` epic** (`issuectl show mcp-integration`): how agents reach
glasspad (an MCP surface over `serve`/`create`/`open`). Start a fresh `/stint`
and plan that round, or pick from the backlog below.

**Two things flagged this round for a human eye (not blocking):**
1. **`glasspad data`** — the old data parsers were NOT deleted; they became an
   opt-in `glasspad data` CLI helper (legacy CSV/JSON/mbox → JSON rows, AI-first
   `--json`/`--format`/`--meta`). Confirm this matches intent vs. dropping them.
2. **orchestratectl worker hang** — Wave 3a's worker Claude process hung mid-run
   (committed, then died before merge). It was salvaged cleanly (green branch
   fast-forwarded, deferred `/llm-review` run after). If this recurs, consider a
   watchdog. Salvage pattern: verify branch green → `git merge --ff-only` →
   `git worktree remove --force` + `git branch -d` → spawn a deferred-review
   spinoff to close the skipped review gate.

**Backlog / decisions (see below):** `structured-api-errors` is now **moot**
(`/api/pads` removed) — close it. `auth-status-codes` likewise moot under
localhost-only — close it. `mcp-integration` epic is the forward work.

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
- [x] **Phase 3 — CLI** DONE (Wave 3 batch 2, `29ac6bf`). `serve ./dir`,
  `create ./file.html`, `open` + fragment-vs-full-document detection; AI-first
  `--json` + strict validation. Removed `src/docs.rs`. **Landed by SALVAGE:** the
  worker's Claude process hung mid-run (confirmed by user), dying after commit but
  before its merge/review. Branch was green (build/clippy/test/31-check suite all
  pass, CLI tests 73→78), fast-forwarded onto `master`, dead worktree/branch
  cleaned up. **Deferred `/llm-review` DONE** (`e04b296`/`49286c3`): 4-model
  review, 9 FIX applied — detector now skips a leading `<?xml?>` prolog + accepts
  form-feed delimiter; reject `--port 0`; validate space name before file I/O;
  structured errors instead of panics; bounded reads. `cargo test` 332 green,
  suite 31 + Wave 2a green, serve/create/open verified.
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
- [x] **Phase 5 — Nav + cross-links** DONE (Wave 4, `1d549a6`/`cc3ff85`/`d240e1c`).
  Trusted-parent nav chrome lists the space's artifacts; shared validated
  `navigateTo(slug)` (grammar + KNOWN_SET allowlist, reused by the bridge) swaps
  the iframe with no reload; full-doc `target=_top`. Titles as `textContent` only
  under Trusted Types; nonce-only shell `script-src`; bidi/control strip + a11y.
  Injection probe added. Suite → 41 browser checks. 4-model review confirmed the
  injection boundary sound.
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
