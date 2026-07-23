# Wave plan — html-artifact-host-rewrite (v0.2, localhost)

> Turns the six phases in `plan.md` §10 into an execution schedule for `/stint`:
> what lands before what, what may run in parallel, the definition-of-done per
> wave, and the merge policy per wave. Read alongside `plan.md` (the *what*) and
> `design.md` (the security model that Wave 1 must implement).
>
> **Cardinal rule:** the security contract + its adversarial browser test
> (Phase 1) land and pass **before** any old code is deleted (Phase 6). Every
> wave branches from the previous wave's *landed* result on `master`.

## Dependency graph

```
        ┌────────────────────────────────────────────────┐
        │ Wave 1 — Phase 1  Security contract + iframe    │  (GATE, interactive)
        └───────────────┬────────────────────────────────┘
                        │ everything branches from here
        ┌───────────────┴───────────────┐
        ▼                               ▼
   Wave 2a — Phase 2             Wave 2b — Phase 4-static
   Space model + serving        base.css / charts.js / manifest.json
   (core routing, security)     (static content under /_gp/v1/)
        │                               │
        └───────────────┬───────────────┘
                        ▼
        ┌───────────────┴───────────────┐
        ▼                               ▼
   Wave 3a — Phase 3             Wave 3b — Phase 4-bridge
   CLI (serve/create/open)      bridge.js (postMessage + theme)
        │                               │
        └───────────────┬───────────────┘
                        ▼
   Wave 4 — Phase 5  Nav chrome + relative-link cross-nav (TRUSTED parent code)
                        │
                        ▼
   Wave 5 — Phase 6  Coupling audit → removals → migration  (LAST)
```

**Why Phase 4 is split.** `base.css`, `charts.js`, and `manifest.json` are
self-contained static content served from the `/_gp/v1/*` route that Wave 1 already
stubs — they touch no shared router logic, so they run parallel to Phase 2.
`bridge.js` is different: it implements the parent↔child `postMessage` contract
(`design.md` §6) and same-space link interception, so it depends on Phase 1's
finalized bridge schema and pairs naturally with Phase 5's nav. It moves to Wave 3.

## Hot files / collision control

The security-sensitive shared surface is the **axum router + server bootstrap**
(route registration, CSP/header middleware, `main.rs`). Wave 1 owns and finalizes
its shape. After that:

- **Wave 2a (Phase 2)** extends the router (asset routes, rescan, SSE) → it owns the
  router file for that wave.
- **Wave 2b (Phase 4-static)** only *fills in file content* behind the already-stubbed
  `/_gp/v1/*` route + registers static includes → expected to be disjoint from 2a.

**Revisit gate (before launching Wave 2):** confirm from Wave 1's landed diff that
`/_gp/v1/*` is a stub route 2b can populate without editing the same router lines 2a
edits. If that can't be confirmed cleanly, **collapse Wave 2 to serial** (2a then 2b)
— a wrong disjointness guess costs a merge conflict on the security-critical router.
Same check gates Wave 3a/3b parallelism (CLI vs client asset — almost certainly
disjoint, but verify 3a doesn't refactor the same route module).

## Waves

### Wave 0 — this plan  ·  *no code*
- **DoD:** `wave-plan.md` committed under the issue. ← you are here.

### Wave 1 — Phase 1: Security contract + iframe shell  ·  **interactive, GATE**
- **Spawn:** `/worktree-code` (foreground, human-reviewed).
- **Scope:** URL topology (`/{space}/{slug}` shell, `/{space}/_c/{slug}` raw content,
  `/_gp/v1/*` **stub**, `/{space}/` entry); artifact-response headers
  (`CSP: sandbox allow-scripts` + egress CSP naming the host + `nosniff` +
  `Referrer-Policy: no-referrer` + `Permissions-Policy` deny-list); null-origin
  iframe; validated `postMessage` bridge (`event.source === iframe.contentWindow`);
  control-plane guards (loopback bind, reject `Origin: null`, `Host` validation).
- **DoD:**
  1. The **adversarial browser test suite** passes: per-channel exfil attempts
     (`fetch`/`sendBeacon`/`<img>`/WebSocket/form-post) blocked by CSP; sandbox-escape
     attempt fails; direct-open of `/{space}/_c/{slug}` is still sandboxed by the
     response header; `postMessage` abuse (wrong `event.source`, oversized/rate) rejected.
  2. **Vega-Lite `'unsafe-eval'` question resolved** — does a real `gp.chart()` render
     inside the artifact CSP? Freeze the exact `script-src` accordingly and record it in
     `design.md` §4.
  3. `cargo build` + `cargo clippy` clean; `./test-browser.sh errors` clean.
- **Merge policy:** **No auto-merge.** User reviews and merges via `/worktree-merge`.
  This is the security gate for the entire rewrite.

### Wave 2 — Space model + static base assets  ·  *2a ∥ 2b (pending revisit gate)*

**2a — Phase 2: Space model + live directory serving** · autonomous + review
- **Spawn:** `/worktree-spinoff --headless` with a brief that **requires
  `/llm-review` (+ `/assess-findings`) before merge** — it touches production
  security code (path-traversal / symlink rejection, MIME + `nosniff`, size limits).
- **Scope:** snapshot scanner + atomic swap; slug grammar + collision/reserved-name
  (`_gp`,`_c`,`assets`,`api`) hard-error rejection; asset routing + MIME detection +
  per-file/per-space size limits; symlink/traversal rejection; filesystem watch + SSE
  reload (narrow `connect-src`); home + title resolution (parsed, not regexed, inserted
  as text).
- **DoD:** malformed/colliding/reserved slugs rejected with informative errors; a
  half-written file is never served (atomic snapshot); traversal + symlink probes
  rejected (add to the adversarial suite); SSE reload refreshes the browser on file
  change; `cargo build`/`clippy` clean.

**2b — Phase 4-static: base.css / charts.js / manifest.json** · autonomous, light review
- **Spawn:** `/worktree-spinoff --headless`. Lower risk (content served *inside* the
  sandbox); a light `/llm-review` is sufficient.
- **Scope:** `base.css` = the existing `--gp-*` design system (preserve `DESIGN.md`,
  light/dark); `charts.js` = thin `gp.chart(el, spec)` over Vega-Lite (honor the
  `script-src` frozen in Wave 1); `manifest.json` describing `gp.chart()`'s signature.
  All under `/_gp/v1/`, `Access-Control-Allow-Origin: *` where CORS-gated.
- **DoD:** a fragment artifact using `gp.chart()` renders a real chart end-to-end;
  base tokens apply in both themes; assets served with correct MIME + CORS headers.

- **Merge policy:** both autonomous/self-merging. Sequence the two merges (2a first,
  then rebase 2b) even when developed in parallel, so the router lands once cleanly.

### Wave 3 — CLI + bridge  ·  *3a ∥ 3b*

**3a — Phase 3: CLI** · autonomous + review
- **Spawn:** `/worktree-spinoff --headless`, brief **requires `/llm-review`** — CLI
  surface must follow `AGENTS-AI-FIRST-CLI.md` (strict validation, `--json`, no
  interactive prompts, informative errors).
- **Scope:** `serve ./dir` (drives Phase 2 live serving), `create ./file.html`
  (one-artifact space), `open <space>`; **fragment vs full-document detection**
  (BOM/whitespace/comment-tolerant, not a naive prefix check).
- **DoD:** all three commands work against a real directory; fragment and
  full-document inputs both detected and served correctly; `--json` + error contract
  per the AI-first doc; `cargo build`/`clippy` clean.
- **Coupling:** resolve `structured-api-errors` (backlog decision) before/with this
  CLI surface — flag to the user in the round that spawns 3a.

**3b — Phase 4-bridge: bridge.js** · autonomous + review
- **Spawn:** `/worktree-spinoff --headless`, brief **requires `/llm-review`** — it is
  the parent↔child security channel.
- **Scope:** intercept same-space **relative** link clicks → `postMessage` parent to
  swap iframe; apply theme on toggle (correct theme still inlined at wrap time for
  no-FOUC); auto-injected into fragment-wrapped artifacts only. Full-document artifacts
  opt in themselves and fall back to `target="_top"`.
- **DoD:** relative-link click swaps the iframe via the validated bridge; malformed
  `postMessage` still rejected (extends Wave 1's postMessage tests); theme toggle works.

- **Merge policy:** both autonomous. CLI vs client-asset are disjoint → parallel, but
  confirm at the revisit gate.

### Wave 4 — Phase 5: Nav chrome + cross-navigation  ·  autonomous + **review (trusted code)**
- **Spawn:** `/worktree-spinoff --headless`, brief **requires `/llm-review`**. This is
  **trusted parent-frame code** (`design.md` §6): artifact-derived text inserted as
  **text, never `innerHTML`**; Trusted Types enabled in the parent CSP; slug resolved
  against the server's artifact table, length + rate bounded. Consider promoting to
  interactive `/worktree-code` if the review surfaces trust-boundary doubts.
- **Scope:** nav chrome rendered in the trusted parent from the space's artifacts (+
  optional `glasspad.yaml` order/grouping); bridge intercepts same-space relative links
  to swap the iframe; full-document fallback via `target="_top"`.
- **DoD:** nav lists the space's artifacts in the right order; clicking navigates within
  the space without a full reload; injected titles/slugs cannot break out of text
  context (add an injection probe to the adversarial suite); `cargo build`/`clippy` clean.

### Wave 5 — Phase 6: Removals + migration  ·  autonomous + review, **LAST**
- **Precondition:** Wave 1's adversarial suite passes **on the new path** and Waves 2–4
  are green. Do not start until then.
- **Spawn:** `/worktree-spinoff --headless`, brief **requires `/llm-review`** and an
  explicit **coupling audit first** (what still imports the "reused" server pieces).
- **Scope:** audit coupling of the reused server/token/`ensure_server`/`open`/CSP infra;
  then delete `src/spec/schema.rs`, `src/spec/validate.rs`, `src/client/dashboard.js`;
  demote `src/security/sanitize.rs` from primary mechanism (keep only if an optional
  static-safe mode wants it); move `src/data/*` to an optional `glasspad data` helper.
  Update `skill.md`, `README.md`, `DESIGN.md` pointers.
- **DoD:** old section-DSL path gone; `cargo build`/`clippy`/tests clean; the Phase 1
  adversarial suite **still passes**; docs point at the new model. ~6000 lines removed.

## Merge-policy summary

| Wave | Unit | Spawn | Review before merge | Merge |
|---|---|---|---|---|
| 1 | Phase 1 | `/worktree-code` | **human (gate)** | user via `/worktree-merge` |
| 2a | Phase 2 | `/worktree-spinoff --headless` | `/llm-review` (security) | self-merge |
| 2b | Phase 4-static | `/worktree-spinoff --headless` | light `/llm-review` | self-merge |
| 3a | Phase 3 CLI | `/worktree-spinoff --headless` | `/llm-review` (CLI contract) | self-merge |
| 3b | Phase 4-bridge | `/worktree-spinoff --headless` | `/llm-review` (trust channel) | self-merge |
| 4 | Phase 5 nav | `/worktree-spinoff --headless` | `/llm-review` (trusted code) | self-merge (maybe interactive) |
| 5 | Phase 6 removals | `/worktree-spinoff --headless` | `/llm-review` + coupling audit | self-merge, LAST |

## Orchestrator checklist per wave
- Branch each wave from the previous wave's **landed** `master`.
- Record every spinoff run id; `orchestratectl run wait` then **git-verify the landing**
  (`git merge-base --is-ancestor <branch> master`) before counting it — run status is
  unreliable (settled ≠ landed).
- Sequence the two merges within a parallel wave (lower-numbered unit first, rebase the
  other) so the shared router lands once.
- One deploy per wave: `cargo build`, restart local server, create a new pad, confirm in
  browser (`./test-browser.sh`, check `errors` first). Wave 1's adversarial suite is the
  gate for the whole rewrite.
- Keep `master` clean between waves; commit issue/status changes immediately.
