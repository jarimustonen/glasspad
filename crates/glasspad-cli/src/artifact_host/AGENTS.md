# `artifact_host`: the sandboxed artifact host

This directory is the HTTP and filesystem side of the promise Glasspad makes: an
agent-written page renders inside a null-origin sandboxed iframe and cannot escape it,
reach another space, or exfiltrate data. The deterministic decisions (the CSP text,
the shell markup, fragment wrapping, markdown rendering, title sanitization, the
content-version hash) live in `crates/glasspad-core/src/artifact_host/`; the modules
of the same name here (`headers.rs`, `shell.rs`, `wrap.rs`, `render.rs`) are thin
re-exports, with `headers.rs` adding only the HTTP-typed `hardening_headers` list. The
code that is actually here is the directory scanner (`space.rs`), the routes and the
atomic snapshot (`mod.rs`), the control-plane guards (`guards.rs`), the deliberately
hostile `demo` fixtures (`fixtures.rs`), and the base libraries served at `/_gp/v1/`
from `assets/`.

The security model is written once, in `issues/html-artifact-host-rewrite/design.md`,
and every module's doc comment explains its own piece well. This file adds what those
do not: which decisions are settled and why, the traps that cost something to learn,
and what a change here puts at stake. `AGENTS-DIAGRAMS.md` beside this file covers
inline SVG in markdown spaces.

## What is at stake

Every artifact is hostile until proven otherwise, because the agent that wrote it can
have been prompt-injected. The viewer's browser is the same browser that talks to the
loopback control API, so a hostile page in another tab is part of the threat model too
(DNS rebinding, cross-origin requests to `127.0.0.1`). Hosted mode adds tenants whose
pages are addressed by capability slugs that no other tenant may learn.

The defence is layered, and each layer does one job (design.md §8). The sandbox without
`allow-same-origin` isolates the DOM and the origin; it does not stop the artifact from
sending requests. The CSP closes egress. The Host guard, applied to every route, and the
`Origin` allowlist on the submit endpoints protect the server independently of both;
`guards::control_origin_guard` is currently unwired. Most reasoning about a change here
comes down to knowing which layer is responsible for the property you are about to
touch. "The sandbox blocks fetch" is a common and wrong belief; `connect-src 'none'`
does that.

The decisions in the next section are Jari's product decisions, made with their
trade-offs in view. Relaxing one, adding a sandbox token, or naming anything new in the
artifact's `connect-src` changes the product promise. That is not something a unit does
in passing because a feature needs it, however small the change looks; it is an issue
with the case for it, and Jari decides. The reverse is also a fork: a tightening that
breaks a shipped capability (Vega charts, full-document `target="_top"` navigation, raw
HTML and SVG in markdown) is the same kind of decision. Inside the boundary you are
free: add probes, tighten what breaks nothing, refactor behind the seams, and verify
end to end before merging.

Security code attracts speculative findings. `TODO.md`'s standing lessons record that a
review finding justified by "another layer already validates this", or one that
requires an attacker who already writes the server's own storage, is not work.

## The frozen boundary and its reasons

The artifact response, at `/{space}/_c/{slug}`, carries the policy built by
`headers::artifact_csp_from_origins` in core. The exact text is in that function and
in design.md §4; these are the reasons behind its shape.

The CSP names explicit origins rather than `'self'`, because `'self'` matches nothing
under a null origin. Loopback names both `127.0.0.1` and `localhost` since browsers
treat them as distinct origins and the iframe `src` is relative, so it inherits
whichever one the user opened. LAN mode adds exactly one opted-in origin, hosted mode
names its single public origin. Only that host list is parameterized; every closure is
identical across run modes.

The sandbox tokens are `allow-scripts allow-top-navigation-by-user-activation`, and no
`allow-same-origin`. The top-navigation token is decision D1 in design.md §10: a
full-document artifact gets no `bridge.js`, so its author navigates between pages with
`target="_top"`, and the click-gated residual was accepted for that.

`script-src` includes `'unsafe-eval'` because Vega-Lite compiles its expression
language with the `Function` constructor; without it `gp.chart()` cannot render. This
was verified, not assumed: in debug builds the content route honours `?csp=noeval`,
which serves the same policy minus that one token, and the adversarial suite proves
that the `Function` constructor Vega-Lite depends on is blocked under it. It is
acceptable because the artifact already runs inline attacker script under
`'unsafe-inline'`; containment was never "can it run JS" but egress plus origin
isolation, and those are untouched.

`connect-src 'none'` is the exfiltration boundary, and it closes requests to the
artifact's own host too. Live reload works because the trusted shell holds the
`EventSource` under its own `connect-src 'self'`. The trap here looks harmless: naming a
single path such as `/_gp/reload` in the artifact's `connect-src`. A CSP path-source
ignores the query string, so `fetch('/_gp/reload?leak=…')` becomes a local egress and
log channel. An earlier iteration did exactly this and was reverted (design.md §4).

`form-action 'none'` and the absence of `allow-forms` are what the return channel
deliberately did not open; see below.

The CSP is a response header, not only an iframe attribute, so a copied link opened in
a new tab is sandboxed as well. A `<meta http-equiv="Content-Security-Policy">` inside
a body can only tighten the effective policy, which is why templates and markdown may
govern the body freely: the server header stays authoritative.

Space assets (`/{space}/assets/*` and the `_c/assets` alias) carry `nosniff` and
`Content-Security-Policy: sandbox`. The `sandbox` directive applies only when the
asset is loaded as a document, so a hostile `logo.svg` opened top-level runs script-less
in a null origin while `<img>`, `<script src>`, and `<link>` from an artifact still
work. Assets carry no `Access-Control-Allow-Origin`: a wildcard would let any web page
the user has open `fetch()` and read a space's assets, because such a request arrives
with a legitimate loopback `Host` and passes the guard.

Fragment wrapping and markdown rendering are not sanitization (design.md §7). Raw HTML
and inline SVG pass through verbatim, and SVG is a scripting host (`<script>`,
`onload`, `<foreignObject>`, URL references). That is safe because of the boundary
above, not because of the format, and "it only contains SVG" is never a reason to
relax anything.

## The trusted shell

The shell at `/{space}/{slug}` is first-party chrome, so `'self'` means something
there. Its `script-src` names only the per-response nonce, not `'self'`: the shell
loads no same-origin script file, and `'self'` would authorize a parser-created
`<script src="/{space}/assets/attacker.js">`, which is agent-authored content on the
same origin, if any markup injection into the shell ever appeared. Trusted Types is
required with no default policy, so any string assigned to an HTML sink throws. That is
the reason all chrome is built with `createElement` and `textContent`, and why
artifact-derived titles reach the shell's script only inside a JSON-for-script data
literal with `<` encoded. The current title's server-rendered copies in `<title>` and
the iframe's `title` attribute are HTML-escaped, the attribute copy with quotes encoded
too, because a stray `"` there could inject a second `sandbox` attribute. The
server-side probe in `test-security.sh` checks the data-literal encoding.

The parent side of the postMessage bridge checks `event.source === iframe.contentWindow`
and not `event.origin`, which is the string `"null"` for every sandboxed frame and
proves nothing. It accepts an exact small schema, rate-caps messages, and rejects
transferred ports. Nav clicks and bridge messages share one validated `navigateTo`
path against the space's slug allowlist; a same-slug navigate is a no-op so a hostile
child cannot loop the parent. The structured-clone cost of a flood cannot be bounded
inside the listener and is an accepted residual (core's `shell.rs` explains).

## The scanner and the snapshot

`scan_dir` reads a directory into an immutable `Space`, all or nothing: a symlink
anywhere in content, a reserved or colliding slug, an oversize file, or an unknown
template name is a hard error and the server refuses to start rather than serve part
of a space. Assets are looked up in the pre-scanned map by exact key, so traversal is
structurally impossible rather than filtered. Size limits are enforced on the bytes
actually read, because a `stat` length can be grown by a concurrent write, and a
rendered markdown body is capped again because markup amplifies. Top-level `AGENTS.md`
and `CLAUDE.md` are skipped as repository metadata; the exception is exact-name and
top-level only.

The `_c/assets` alias exists because a browser resolves an authored `assets/x` against
the iframe's `_c/{slug}` URL. It is the same asset handler over the same map; the point
of the arrangement is that no `_c` path ever becomes a filesystem read.

`ArtifactHost` swaps snapshots atomically, and each handler captures one `Arc` up front,
so a mid-request rescan never mixes a title from one snapshot with nav from another. A
space present in the snapshot is served only from it; a missing slug does not fall
through to the `demo` fixtures. A rescan that fails keeps the last good snapshot
serving. The watcher is a dependency-free 500 ms poll in `server.rs`.

Page slugs may contain up to four individually validated segments (256 bytes
in all); space names and hosted capability slugs remain flat. Only manifest-named
nested trees are scanned, and missing nested members fail explicitly; without a
manifest the old flat-only scan remains. A scanned page directory rejects symlinks,
except exact `AGENTS.md` and `CLAUDE.md` instruction entries. Hosted generations
store nested pages under `artifacts/` and revalidate all paths on reload. The flat
`Space.nav` is the complete slug allowlist. Manifest `groups:` are reconciled
against it at scan time and again on hosted ingest, because the wire format is
untrusted. Companion nesting is declared in the manifest; Glasspad does not parse
dotted file stems into hierarchy, by decision.

## The return channel and rounds

The artifact stays frozen and the shell is the airlock. `gp.submit(data)` in
`bridge.js` posts a message to the shell, and the shell POSTs it to the server
(`/{space}/_gp/submit` on loopback, `/api/v1/pages/{slug}/submit` hosted). The server
binds the submission's space or slug, owning tenant, and content-version from the
trusted request context, never from the payload; the endpoint is `Origin`-allowlisted
and size and rate capped; a submission carrying a stale content-version is a `409`. A
hosted submit hands back a one-submission random status token, and every wrong
combination of token, page, and tenant is the same opaque `404`, so the public status
route is not an existence oracle. Agents read submissions by poll, long-poll, or SSE
stream over one persisted cursor; the stream has its own held-connection budget so
open streams cannot starve the long-poll, and the artifact CSP names none of these
paths.

Multi-round reuses the one reload `EventSource` rather than a new push channel. A
`round` event carries no URL, only the space, content-version, and round id, so the
most it can do is make a shell re-fetch its own content route. Delivery is scoped
server-side by `?space=` on the stream, because an unscoped global broadcast would hand
every connected viewer other tenants' capability slugs; the client-side space check is
defence in depth. Hosted rounds are stored as immutable generations behind an
atomically swapped pointer so a crash mid-push keeps the previous round. The store is
`crates/glasspad-cli/src/submissions.rs`, the handlers are `hosted/submit.rs`,
`hosted/rounds.rs`, and `server.rs`, and the design record is
`issues/artifact-return-channel/`.

## Verifying a change

`cargo test artifact_host` covers the header contract, grammar, guards, the scanner
(`space::fs_tests`), and the atomic-swap concurrency test. `./test-security.sh` is the
executable form of the contract: browser probes in `tests/security/run.mjs` prove that
Chromium enforces the policy, and the shell script's own probes cover what a browser
cannot help with (traversal, symlinks, CSRF, rate limits, tenant scope, LAN Host
handling). The root `AGENTS.md` explains how to read its result and why a partial run
once passed for green.

Two things about the suite are specific to this directory. The `demo` fixtures are the
targets the browser probes drive, so they stay and grow rather than getting cleaned up.
And when you add attack surface, add the probe on the side that enforces the property:
the browser for anything the CSP or sandbox does, the server for anything the scanner,
guards, or handlers do. Check counts written in documents drift; the script's summary is
the truth.

The base libraries under `assets/` are compiled into the binary and served through the
match in `fixtures::gp_asset`. `glasspad build` bundles `fixtures::BASE_LIB_NAMES`,
which is that set minus the test-only `probe.js`, and resolves each name through
`gp_asset`. A new base library needs an entry in both; do not introduce a third list.
