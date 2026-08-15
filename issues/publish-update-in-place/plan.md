# Plan — publish update in place (`--update <slug>`)

## Gap

The store already supports in-place space update via a stable `--space-key` (a
forethought caller-chosen key mapped to a slug). What's missing is the flow the
reporter hit: **already published, got `/p/<slug>/`, now update THAT URL** — no
key was set up front. `--idempotency-key`/`--space-key` can't retro-target an
existing slug you only know by its capability URL.

## Surface (AI-first, strict)

`glasspad publish <path> --update <slug>` (hosted-only).

- Targets an existing space by its capability slug and **replaces its content in
  place**, preserving the URL. Returns `200` (`created:false`).
- **Fails** (opaque `404 no_such_space`) if the slug is missing OR owned by
  another tenant — distinct from `--space-key`, which falls through to a fresh
  mint. You are naming a specific existing resource; a miss is an error, not a
  create.
- Mutually exclusive with `--space-key` (clap `conflicts_with`) — two ways to say
  "update in place"; picking both is a usage error.
- On a loopback target it joins the hosted-only offenders rejection.
- Flag-only (not config/env): it's a per-invocation target, not a persistent
  identity like `space_key`.

## Server

New endpoint `PUT /api/v1/spaces/{slug}` (same auth + body-limit as the space POST):

- Validates slug grammar at the boundary → opaque 404.
- Early owner-scope via new `Store::space_tenant(slug)` (reuses `owner_from_meta`)
  so a non-owner does no build work and learns nothing.
- Builds the bundle through the SAME `build_space_bundle` seam as POST.
- `Store::update_space(tenant, slug, space) -> Result<PublishedSpace, UpdateError>`:
  - re-checks ownership under the mutation lock (authoritative, TOCTOU-safe),
  - preserves `created_at` (retention clock unchanged), stamps `updated_at`,
  - `materialize_space(replace=true)` — the existing atomic staged replace,
  - clone-modify-swaps the served snapshot.
- `UpdateError { NoSuchSpace, Io }` → 404 / 500.

POST `/api/v1/spaces` (create / create-or-update-by-key) semantics are untouched.

## Tests

- store: successful in-place update (body swaps, slug + created_at preserved,
  updated_at advances); foreign-tenant → NoSuchSpace; missing slug → NoSuchSpace;
  page-tree-only slug → NoSuchSpace.
- http (mod tests): PUT updates 200 + body swaps; foreign key 404; unknown slug
  404; bad-grammar slug 404; unauth 401; frozen CSP unchanged after update.
- idempotency/space_key behaviour unchanged (existing tests stay green).

## Docs

`skill.md` publish section + the `Publish` clap help get `--update`.
