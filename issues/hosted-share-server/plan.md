# Hosted share-server — implementation plan (0.3.0)

Security-critical. A second run mode beside the loopback live server: a long-lived
**hosted share server** many agents push pages to over the network (API-key auth),
serving them at unguessable capability-slug public URLs — plus a `glasspad publish`
client. This plan is the design gate; it is committed before any implementation.

The shape is **settled** in `item.md` (mirrors the proven publish-html client/server)
and is not relitigated here — this document only says *how* it is built and, above
all, *why the security boundary is preserved*.

## 1. Two run modes, one artifact host

The loopback `serve`/`create`/`render` path in `src/server.rs` is **unchanged**:
same loopback bind (`127.0.0.1`), same global `guards::host_guard` DNS-rebinding
defense, same tests. Nothing in this feature weakens or refactors that path.

The hosted mode is a **separate run mode** (`src/hosted/`) that reuses the exact
same rendering/sandbox seam (`artifact_host::{wrap, shell, headers, space, render}`)
but supplies its own **router, storage, auth, and slug** layers. It never binds
loopback and never touches `host_guard`.

The one shared-code change is a *pure parameterization* (see §7): the artifact
CSP's named origin and the shell's URL prefix become configurable so the identical
handlers serve either a loopback origin or a public origin. Every frozen decision
— `sandbox allow-scripts` with **no** `allow-same-origin`, `connect-src 'none'`,
Trusted Types on the shell, the hardening headers — is byte-identical in both modes.

## 2. URL topology (hosted)

```
POST /api/v1/pages          — ingest (auth required): publish one immutable page
GET  /p/{slug}/             — trusted shell framing the page's single artifact
GET  /p/{slug}/_c/{slug2}   — raw artifact document (carries the sandbox CSP)
GET  /p/{slug}/assets/{*p}  — the page's static assets (MIME + size limits)
GET  /_gp/v1/{*path}        — pinned base libraries (base.css / bridge.js / …)
GET  /_gp/reload            — SSE stream (present but never fires; pages are immutable)
GET  /healthz               — liveness (no auth, no data)
```

`/api`, `/_gp` are already **reserved** space names (`artifact_host::RESERVED`), so a
capability slug can never collide with them. The published space's read routes live
under the `/p/` mount; the shared handlers emit `/p/{slug}/…` URLs via the new mount
prefix (§7). `/_gp/*` stays at root in both modes, so `wrap.rs`'s hardcoded
`/_gp/v1/base.css` + `/_gp/v1/bridge.js` and the shell's `/_gp/reload` need no change.

A published page is a **single-artifact space** (exactly what `create`/`render`
already produce via `one_artifact_snapshot`): the capability slug is the space name,
the lone artifact is `index`.

## 3. Ingest auth model (the write surface)

**Transport:** `POST /api/v1/pages` with `Authorization: Bearer <api-key>`.

**Key source (operator):** `--api-key-file <path>`, loaded once at startup into an
immutable in-memory table. File format, one entry per non-blank/non-`#` line:

```
<tenant-id>:<api-key>
```

`tenant-id` matches the space grammar (`[a-z0-9-]`, ≤64). `api-key` is opaque but
strictly validated at load (AI-first §1, **fail-fast**): reject empty keys, keys
shorter than `MIN_API_KEY_LEN` (32 chars — the operator's responsibility to make
them high-entropy, but we refuse trivially-weak ones), duplicate keys, and malformed
lines, each with an informative error naming the offending line number. An empty or
unreadable key file is a **hard startup error** — the server does not come up with an
authenticatable-by-nobody or authenticatable-by-anybody ingest surface (fail-closed).

**Verification (per request), fail-closed:**
1. No `Authorization` header, or not `Bearer <token>` → `401`, no body detail.
2. Extract the presented key; compare it against **every** stored key with a
   **constant-time** comparator (`security::ct_eq`: for equal-length inputs the
   compare time is independent of *where* the bytes differ, so a timing channel
   cannot walk the key out; a length mismatch returns early — the length of a
   high-entropy ≥32-char key is not treated as a secret). Iterate all keys without
   short-circuit so timing does not reveal table position. On a match, the request
   is attributed to that key's `tenant-id`; on no match → `401`. (A huge presented
   token is cheap: the length-mismatch early-out makes each compare O(1).)
3. Only an authenticated request reaches the ingest handler; the tenant id is
   carried in a request extension, never taken from client-supplied data.

Empty presented token, whitespace token, wrong scheme, and absent header all fail at
step 1/2 → `401`. There is **no** fallback that treats a parse failure as success.

## 4. Capability-slug scheme (unguessable, enumeration-resistant)

`slug::generate()` draws **16 cryptographic random bytes** (128 bits) from the OS
RNG and encodes them as lowercase RFC-4648 base32 (`a-z2-7`), yielding a 26-char slug
that satisfies the existing `valid_name` grammar (starts alphanumeric, `[a-z0-9-]`,
≤64). 128 bits of entropy makes enumeration/guessing infeasible (there is no read
auth by design — "hold the link" — so the slug *is* the capability). On the
astronomically unlikely chance of a collision with an existing stored page, the store
regenerates. Slugs are never derived from content (no oracle), never sequential, and
never logged at a level that would leak them into shared logs beyond the tenant's own
line.

## 5. Multi-tenant isolation

The isolation guarantee is **structural**, not policy-enforced-at-read:

- **Immutable pages.** A page is written once and never mutated. There is **no**
  update/overwrite/delete API. `publish` always mints a **new** random slug. So one
  tenant *cannot overwrite* another's page — there is no code path that writes to an
  existing slug. Cross-tenant write is impossible by construction.
- **Unguessable slugs.** Reads are public-by-design (capability URLs), but a tenant
  cannot *enumerate or guess* another tenant's slugs (128-bit, §4). There is no
  "list my pages" surface in this iteration that could leak the set.
- **Owner tag.** Each stored page records its `tenant-id` in `meta.json` for GC,
  audit, and a future scoped-list feature. It is set from the authenticated tenant,
  never from the request body.
- **No ambient authority.** The read side grants **zero** privilege from origin or
  cookies — it returns only the sandboxed public artifact. So even a DNS-rebinding or
  cross-origin attacker gains nothing a plain `GET` of the (already public) URL would
  not (see §8).

## 6. Storage + retention/GC

**Layout** under `--store <dir>`:

```
<store>/pages/<slug>/artifact.html   — the raw artifact body (fragment or full doc)
<store>/pages/<slug>/meta.json       — { schema, slug, tenant, title, kind, created_at }
```

- **On startup:** scan `<store>/pages/*`, load each page (bounded read, same
  `MAX_FILE_BYTES` cap; skip + log any corrupt/oversize entry rather than aborting the
  whole server), and build the initial `Snapshot` (slug → single-artifact space).
- **On publish:** write `artifact.html` + `meta.json` to a fresh `<slug>/` dir
  (write to a temp path then atomic `rename`, so a crash never leaves a half-page a
  reader could see), then insert the space into the live snapshot via the existing
  atomic `ArtifactHost::swap` (readers in flight keep the old `Arc`).
- **Retention/GC:** a background task (hourly tick) removes page dirs whose
  `created_at` is older than `--retention-days` (default **90**), then rebuilds and
  atomically swaps the snapshot so an expired page is **no longer served** (a request
  to a GC'd slug 404s). GC also drops a page from the in-memory snapshot before/with
  removing it from disk, so there is no window where the file is gone but the snapshot
  still serves it, nor vice-versa that matters (serving a page whose file is deleted
  is harmless — bodies are in memory — but the snapshot swap closes it promptly).
- **Caps:** per-page body cap (`space::MAX_FILE_BYTES`), and a total-pages cap
  (`MAX_PAGES`, default 100_000) so ingest cannot unbound disk/memory; over the cap →
  `507`/`429`-style rejection with an informative envelope. Per-tenant rate/quota is
  noted as future work (operator can front with a proxy); the global cap is the
  hard backstop this iteration ships.

## 7. Reusing the sandbox/CSP boundary unchanged (the core security argument)

The published artifact body flows through the **exact same** seam as a locally-served
one: `one_artifact_snapshot` → `artifact_content` handler → `wrap::render_artifact` →
frozen `headers::artifact_csp*` + `headers::hardening_headers`, framed by
`shell::render` under `headers::shell_csp` (nonce + Trusted Types). The hosted mode
**adds no header, widens no directive, and removes no sandbox token.** Concretely:

- **Origin parameterization.** `headers::artifact_csp(port, allow_eval)` is refactored
  to delegate to `artifact_csp_from_origins(origins: &str, allow_eval)`; the loopback
  wrapper passes `self_origins(port)` and is byte-identical (its existing unit tests
  are unchanged and keep passing). The hosted mode passes the operator's single
  `--public-host` origin (e.g. `https://pad.example.com`). This only changes *which
  host the artifact may load its own `/_gp/v1/*` script/style from and be framed by* —
  the null-origin sandbox, `connect-src 'none'` exfil boundary, `object/frame/
  worker/base/form` closures, and `img-src …data:` are **identical**. `'self'` remains
  meaningless under the null origin in both modes, which is exactly why the origin is
  named explicitly.
- **Mount prefix.** `shell::render(..., mount)` gains a mount prefix applied **only**
  to the `/{space}/…` content + nav URLs (`{mount}/{space}/_c/{slug}`), not to
  `/_gp/*`. Loopback passes `""` (byte-identical output, tests unchanged); hosted
  passes `/p`. The prefix is a fixed server constant, never client input.
- **`shell_csp` is unchanged** — it uses `'self'`, which is correct under any origin,
  so trusted-chrome CSP + Trusted Types are literally the same function.
- **Ingest cannot widen the boundary.** The artifact body is untrusted, exactly like
  a `create`d/`render`ed body today: a `<meta http-equiv=CSP>` in the body can only
  *tighten* (the real CSP is the server-set response header), and the trusted shell is
  a different route built from the resolved title via `textContent`/JSON-for-script,
  so a hostile body can neither widen the CSP nor inject the shell. The existing
  `hostile_template_body_cannot_widen_csp` / `rendered_hostile_heading_is_inert_in_shell`
  tests already prove this for the shared seam; hosted adds an equivalent over its
  own route.

Because public read only ever returns this sandboxed, egress-closed, null-origin
artifact, **public read with no auth is safe** — that is the whole basis of the
decided model.

## 8. Why loopback's DNS-rebinding guard is not needed (and not weakened) here

`host_guard` exists to stop a hostile web page that rebinds its own DNS name to
`127.0.0.1` from reaching a *loopback* server the browser treats as same-origin-ish
and that might expose local privilege. The hosted server is different in kind:

- It **wants** to serve a public host, so a loopback-only Host allowlist is wrong for
  it; the loopback path keeps its guard untouched (its tests still pass — regression-
  checked).
- It grants **no privilege by origin or ambient credential**: read is public and
  returns only the sandboxed artifact; write requires a `Bearer` token in the
  `Authorization` header (never a cookie, never inferred from origin). A rebinding /
  cross-origin attacker therefore gains nothing: they cannot forge the bearer token,
  and reading a public URL is already allowed for anyone holding the (unguessable)
  link.
- Defense-in-depth: the hosted server still applies a **fixed public-host allowlist**
  (`--public-host`'s host[:port]) via a lightweight Host check on all routes, so it
  answers only under its configured name; a mismatched Host is rejected. This is
  configuration correctness, not the loopback rebinding mechanism, and it is separate
  code from `guards::host_guard`.

## 9. CLI surface (AGENTS-AI-FIRST-CLI.md)

### `glasspad host-serve` (run the hosted share server)
```
glasspad host-serve \
  --bind <ip:port>            required. public bind, e.g. 0.0.0.0:8080 (explicit,
                              never defaulted to a routable address silently)
  --public-host <origin>      required. canonical public origin for the artifact CSP
                              and returned URLs, e.g. https://pad.example.com
  --api-key-file <path>       required. operator key file (§3); fail-closed if
                              missing/empty/malformed
  --store <dir>               required. storage root (§6)
  --retention-days <n>        default 90 (>=1)
  [--json]
```
Strict validation up front; fail-fast on a bad origin, unreadable/empty key file, or
un-createable store. Long-running: prints a startup envelope (`serving`, `bind`,
`public_host`, `pages`, `retention_days`) then serves until killed. Structured error
envelope + meaningful exit code (1 user / 2 system) on failure; no interactive prompts.

### `glasspad publish` (client)
```
glasspad publish <file> \
  --server <url>              flag > $GLASSPAD_SERVER > config file (§ AI-first 8)
  --api-key <key>             flag > $GLASSPAD_API_KEY > config file (never logged)
  [--markdown [--template <ref>]]   render markdown+template client-side-equivalent?
                              NO — the server renders; client sends markdown+template
                              fields and the server runs the shared render path.
  [--title <t>] [--json] [--no-open]
```
Reads config from `~/.config/glasspad/config.yaml` (`server`, `api_key`) with
flag/env override. POSTs the artifact to `<server>/api/v1/pages`; on success prints
`{slug, url}` (the data channel) and optionally opens the URL. The api key is read
from config/env/flag and **never** echoed into stdout/stderr/logs. HTTP+TLS via
`reqwest` (rustls). Strict: a missing file, unreadable config, absent server/key,
non-2xx response each surface as an informative error envelope.

## 10. Test plan (adversarial — public network + auth)

Unit + HTTP tests (all must be green; extends `./test-security.sh` where a browser is
needed):
- **Auth fail-closed:** missing header, empty bearer, whitespace bearer, wrong scheme,
  wrong/too-short key, key with trailing junk → all `401`; a valid key → `201`.
- **Constant-time compare** correctness (length-independent, rejects empty/short).
- **Cross-tenant write denied:** immutability means no overwrite path exists — a test
  asserts publish always yields a fresh slug and there is no route that writes an
  existing slug (belt-and-braces: attempting to `POST` a chosen slug is ignored).
- **Slug enumeration resistance:** generated slugs are 128-bit base32, distinct across
  many draws, and pass `valid_name`; a guessed/sequential slug 404s.
- **Loopback guard unregressed:** `server::tests` (host_guard accept/reject) stay green
  verbatim; a hosted test proves the hosted router does **not** carry `host_guard` but
  the loopback `build_app` still does.
- **Ingest can't widen CSP / escape sandbox:** a hostile published body (`<meta CSP>`,
  inline `<script>fetch(evil)>`, stray `</body>`) is served under the *identical*
  frozen artifact CSP (`sandbox allow-scripts`, `connect-src 'none'`, public origin
  named) — asserted on the hosted `_c` route.
- **Retention/GC:** a page past retention is removed from disk **and** 404s on read
  after the GC swap; a fresh page survives.
- `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
  `./test-security.sh` (41 checks + Wave 2a) all green.

## 11. Review

`/llm-review` (auth-bypass / tenant-isolation / rebinding-SSRF weighted) then
`/assess-findings`; apply mechanical/confirmed fixes. Self-merge only if fully green
and no genuine fork remains; a genuine security fork is recorded as a `discussion_items[]`
entry and surfaced, not guessed.

## 12. Deliberate deferrals (not in this iteration)

- Read auth / accounts (explicitly "a later feature" in the model).
- Per-tenant quotas/rate-limits (global `MAX_PAGES` cap ships; operator can front with
  a proxy).
- Scoped "list my pages" API (would need read auth to be safe).
- Custom/vanity slugs (would reintroduce a guess/collision oracle).
