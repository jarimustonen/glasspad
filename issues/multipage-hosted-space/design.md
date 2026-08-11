# Gap 1 — Multi-page hosted publish (space ingest) — design

Status: design → implementation. Scope: **Gap 1 only** (a hosted *space* of
`.html` artifacts). Gap 2 (markdown-native `.md` directories) is out of scope
(`markdown-native-spaces`).

## Problem

`publish <FILE>` and `POST /api/v1/pages` are strictly single-file: each file
becomes an independent single-artifact space under its own capability slug
(`/p/<slug>/`, one artifact keyed `index`). A producer with a directory of linked
`.html` artifacts (e.g. a 62-page docsite that `glasspad build` already renders
locally) has **no way to host the whole space** on `host-serve` with in-space
bridge nav + cross-page relative links (`href="./other"`) resolving — because each
page lands under a *different* random slug, so `./other` resolves to
`/p/<this-slug>/other`, which does not exist.

## Key insight — the read path already does multi-page

The hosted read routes **reuse** `artifact_host::spaces_router` nested under `/p`
(`hosted/mod.rs`). That router already serves a *multi-artifact* `Space`:

* `/p/<space>/` — trusted shell for the home artifact
* `/p/<space>/<page>` — shell for a page
* `/p/<space>/_c/<page>` — the sandboxed content route
* `/p/<space>/assets/<path>` — static assets

with the bridge nav (server-resolved `(slug, title)` table), in-place iframe swaps,
and relative-link resolution — **identical to local `serve`**. The only reason
hosted publish is single-page is that `store.rs` forces every page into a
single-artifact space via `one_artifact_space()`.

So Gap 1 is: **let the store persist and serve a multi-artifact `Space` under one
capability slug.** No new routing, no CSP change, no sandbox change — the read
seam is untouched. This is the lowest-risk shape and keeps the security invariant
(every page a null-origin sandboxed iframe, `connect-src 'none'`) automatically.

## Namespace / URL model

A hosted space is a multi-artifact `Space` stored under **one capability slug**
`<space>` (26-char lowercase base32, 128 bits from the CSPRNG — the *same*
`slug::generate()` as single-file). Pages address as `/p/<space>/<page>`; the space
slug **is** the capability ("hold the link to the space"). All pages of a space
share that one capability — which is correct: they are one published unit, and
cross-page nav stays within the same capability/origin.

Single-file `publish <FILE>` is unchanged and remains the degenerate case (a space
whose one artifact is `index`) stored in the existing `pages/` tree. Space publish
is a **new, separate** surface (`POST /api/v1/spaces`, `glasspad publish-space`).

### Why relative links + bridge nav resolve without touching the sandbox

Nothing new is needed. Because a hosted space is now a real multi-artifact `Space`
served by the existing `spaces_router`:

* A **fragment** page gets `bridge.js` injected (existing `wrap.rs` path). The
  bridge intercepts same-space relative-link clicks and asks the trusted parent to
  swap the framed artifact via the validated `navigateTo(slug)` (grammar +
  `KNOWN_SET` allowlist). The slug set is the space's own pages — cross-space
  navigation is structurally impossible (the target must be a known slug *in this
  space*).
* A **full-document** page uses `<base target="_top">` for inter-page links (the
  existing D1 top-nav path) so a click breaks out to `/p/<space>/<other>` at the
  top level rather than nesting an `x-frame-options: DENY` shell.

Every page is still served on `/p/<space>/_c/<page>` under the **frozen artifact
CSP** (sandbox, `connect-src 'none'`, named public origin). Navigation moves the
viewer between pages *of the same space and origin*; it opens no network egress and
crosses no tenant boundary. The bridge/nav machinery and its adversarial suite
(Wave 3b/4) already prove a hostile page cannot turn nav into an exfil/escape
channel — that proof carries over unchanged because the machinery is unchanged.

## Storage model

A **separate `spaces/` tree**, parallel to `pages/`, so the single-file path is
untouched byte-for-byte:

```
<root>/spaces/<slug>/
  meta.json                { schema, slug, tenant, title?, nav[], home, created_at, updated_at }
  artifacts/<page>.html    the raw artifact bodies (one per page)
  assets/<rel...>          static assets (mirrors the space-relative asset keys)
<root>/space-idem/<tenant>/<sha256(space_key)>.json   stable-key → slug mapping
```

* **Load**: `scan_spaces()` reads each `spaces/<slug>/` back into a `Space`
  (artifacts + assets + nav + home + title), re-validating every field (slug
  grammar, tenant, per-file/space caps, asset-path grammar) exactly like the
  page loader — a hand-tampered store can never smuggle a bad entry into the router.
* **Snapshot**: pages and spaces share the one in-memory `Snapshot.spaces` keyed
  by slug (a page *is* a single-artifact space in the snapshot today). `scan_disk`
  merges both trees. `fresh_slug` checks the snapshot **and both on-disk trees** so
  a page and a space can never collide on a slug.
* **GC**: retention GC reaps expired `spaces/<slug>/` directories (by
  `meta.created_at`) and sweeps dead `space-idem` mappings, on par with `pages/`.
  Same hourly cadence, same startup sweep, same mutation lock (space publish, page
  publish, round push, and GC all serialize on the one `Store::mutation` lock, so
  no read-clone-swap loses another's update).
* **Isolation**: `meta.tenant` records the authenticated owner (server-side, never
  from the body). Reads are public-by-capability (unguessable slug); the stable-key
  mapping is per-tenant-directory scoped **and** records + re-checks the owning
  tenant, identical to the page idempotency layering. Cross-tenant/cross-space
  access is an opaque 404 (no existence oracle).

## Stable space slug + re-publish (the deliberate fork)

The issue asks for "an `--idempotency-key`-style stable space slug so re-publish
updates in place." There are two candidate semantics:

1. **Exactly-once (page semantics today):** a repeat with the same key returns the
   *first* space unchanged; a new body is ignored.
2. **Update-in-place:** a repeat with the same key **replaces** the space's content
   under the same slug/URL.

**Decision: update-in-place, keyed by `--space-key`.** The driving use case is a
docsite whose markdown/HTML sources change and get re-published; the producer wants
the *same URL* to reflect the new content. Exactly-once (returning stale content on
re-publish) would defeat that. This is a considered divergence from single-file
page immutability, and it is safe: the replace is **owner-scoped** (the mapping and
the target space's `meta.tenant` must both equal the authenticated tenant), so a
tenant only ever replaces *its own* space. Without a `--space-key`, each publish
mints a fresh slug (no implicit clobber). This mirrors `push_round`'s "owner
re-renders its own live page in place", generalized to a whole space.

> Recorded as a discussion item: whole-space content under a stable key is
> *mutable in place*, unlike single-file pages which are immutable. Reasonable
> people could prefer immutable-versioned spaces. Chosen update-in-place because it
> is what the stated docsite workflow needs and it stays within the owner's own
> tenant. Revision path if versioned hosting is later wanted: a new `--version`
> axis under the same slug.

On-disk update is a crash-safe atomic directory swap: stage the new space in
`.<slug>.tmp/`, fsync, then `rename(final → .<slug>.old)`, `rename(tmp → final)`,
remove `.old`. A crash mid-swap leaves either the old or new tree intact and a
reclaimable `.<...>` staging/backup dir that GC reaps.

## Transport — the space bundle

The producer's directory must reach the server. The CLI **scans the directory
locally with the exact same `space::scan_dir`** (reusing all its validation:
slug grammar, reserved names, symlink/traversal rejection, caps, MIME, title
resolution, manifest nav/title) and sends a **JSON bundle**:

```json
POST /api/v1/spaces        (Bearer auth, same as /api/v1/pages)
{
  "pages":  [ { "slug": "index", "html": "<...>" }, ... ],
  "assets": [ { "path": "logo.svg", "content_base64": "..." }, ... ],
  "nav":    ["index", "guide", ...],     // optional; server reconciles
  "title":  "My Docsite",                // optional
  "space_key": "tilictl-spec-v1"         // optional stable key (update-in-place)
}
```

The **server re-validates everything** (never trusts the client): each slug via
`valid_name` + reserved check, dedup, per-file + per-space byte caps, entry-count
cap, asset-path grammar, UTF-8. A new `space::build_space_bundle()` applies exactly
the same rules `scan_dir` applies, but over in-memory bundle data, and returns a
`Space`. So the security-sensitive validation has **one** implementation of the
rules, exercised from both the filesystem scanner and the ingest bundle.

Body cap: the space ingest route gets a larger `DefaultBodyLimit`
(`MAX_SPACE_BYTES` + base64 slack) than the single-page route, still bounded.

## `--json` envelopes (AI-first §10)

Space publish returns a superset envelope, never breaking the single-file shape:

```json
{ "schema_version": N, "slug": "<space>", "url": ".../p/<space>/",
  "pages": [ { "slug": "index", "title": "...", "url": ".../p/<space>/" }, ... ],
  "page_count": 62, "created": true, "warnings": [] }
```

`created` is `true` for a fresh slug, `false` when a `--space-key` updated an
existing space in place. The single-file `publish` envelope is untouched.

## Backward compatibility

* `publish <FILE>` → `POST /api/v1/pages` → `pages/` tree: **byte-for-byte
  unchanged**. No struct, route, or on-disk change on that path.
* All existing tests + the 48-check security suite continue to pass; new tests +
  a new space Wave are **added**, nothing removed.

## Security gate (what the new Wave proves)

Added to `test-security.sh` (hosted section): publish a small multi-page space,
then assert —
1. Each page serves under the **frozen** artifact CSP (`sandbox allow-scripts …`,
   `connect-src 'none'`, no `allow-forms`, public origin named) — bundle content
   cannot widen it (a hostile `<meta CSP>` page in the bundle stays contained).
2. In-space relative nav resolves to sibling pages of the **same** space slug; the
   bridge/nav only knows this space's slugs (no cross-space target).
3. **Cross-space isolation**: a second space (different slug) is independent; a
   page slug from space A is a 404 under space B; a bogus space slug is an opaque
   404. Nav/relative links cannot address another space.
4. A different tenant cannot update-in-place another tenant's space (`--space-key`
   collision across tenants → separate spaces; opaque 404 on cross-tenant replace).
5. Retention GC removes an expired space and stops serving it (parity with pages).
