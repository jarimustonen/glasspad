---
name: glasspad
description: Show rich visual HTML views (dashboards, charts, interactive UIs) to the user in their browser. Use when asked to visualize, plot, chart, dashboard, or "show me" something.
cli_version: "0.6.0"
schema_version: 1
---

# Glasspad — hand it markdown, get a URL

**One verb: `glasspad publish <path>`.** Give it a Markdown (or HTML) file — or a
directory of them — and it returns a **URL** the user opens. Where that URL lives
is decided by config, not by you choosing a command: a `target` of `loopback`
(serve on this machine, the zero-config default) or `hosted` (upload to a share
server, return a public link). You author content; `publish` handles the rest.

```bash
glasspad publish ./report.md        # → a URL (loopback by default; hosted if configured)
glasspad publish ./dashboard/       # a directory of pages → one multi-page space
```

Every command takes paths as arguments, emits a stable `--json` envelope, and
fails with an informative error (never an interactive prompt).

## The model

- **Markdown is the standard input.** Hand glasspad `.md`/`.markdown` and it renders
  automatically through a built-in theme. `.html` works too (served verbatim).
- A **single file is a one-page space**; a **directory is an N-page space**.
- A **space** is a URL namespace holding one or more **artifacts** (pages). Each
  artifact is addressed by a **slug** = its filename stem (`sales.md` → slug
  `sales`). Link between pages with ordinary relative links (`<a href="./detail">`).
- Pick the Markdown theme per space in an optional per-space `glasspad.yaml` with
  `template: prose` (default reading theme), `template: dashboard` (card look), or
  a relative path to a producer-owned fragment template such as
  `template: templates/brand.html`. A custom template has exactly one `{{content}}`
  slot and is applied to every Markdown page; it must be a regular UTF-8 fragment
  inside the space (no symlinks, traversal, or full HTML document). It is rendered
  into the uploaded page bodies, so hosted spaces are self-contained. `.md` and
  `.html` pages coexist; a `.md` and `.html` sharing a stem is a hard collision.

## Where it lands: the `target`

`publish` resolves its target from config, **per key**, first file that sets a key
wins:

1. **`.glasspad.yaml`** in your repo (found by walking up from the working dir).
   This is the repo-local config — distinct from the per-space `glasspad.yaml`
   (which is structure only: nav/title/theme).
2. **`~/.config/glasspad/config.yaml`** — the home config.
3. **Built-in default** — `target: loopback`. So with **no config at all**,
   `publish` just serves loopback. Zero-config local works out of the box.

Because the merge is per key, a repo can set only `target`/`favicon` and inherit
`server` + `api_key` from the home config.

```yaml
# .glasspad.yaml (repo root) — the keys publish reads
target: hosted                 # loopback (default) | hosted
server: https://pad.example.com
api_key: sk_live_…             # inline, OR an indirection (below)
template: prose                # default template for markdown pages
space_key: my-docsite          # hosted: stable slug → idempotent re-publish
```

**API-key indirection.** `api_key` accepts an env var or a key file, not only an
inline secret — keep plaintext out of the file:

```yaml
api_key: { env: GLASSPAD_API_KEY }     # read from the environment at publish time
api_key: { file: /run/secrets/gp-key } # read from a file (or: api_key_file: <path>)
```

A relative `file:` path resolves against the **config file's** directory, not the
working directory. Keep credentials in your **home** config: if a repo's
`.glasspad.yaml` sets `server:` while the key comes from your home config,
`publish` warns loudly (a cloned/untrusted repo could redirect your key) — pass
`--server`/`--api-key` explicitly to confirm.

- **`target: loopback`** → serves the space live on `127.0.0.1` (keeps the
  DNS-rebinding Host guard), opens the browser, and **live-reloads** on file edits.
  Runs until killed — start it backgrounded. The private "show me while I work" view.
- **`target: hosted`** → uploads the space and returns a public capability-slug URL
  (`/p/<slug>/…`, `noindex` — "hold the link"). A snapshot; re-run `publish` to
  update it. Two ways to keep the **same URL** across edits (a "living" doc shared by
  link):
  - `--space-key <k>` (or config `space_key:`) — set once; every publish with that
    key updates **in place**. Create-or-update: the first publish mints the slug,
    later ones replace it. Best when you plan the living doc up front.
  - `--update <slug>` — you already published and have the `/p/<slug>/` link; replace
    that exact space in place, keeping the URL. Owner-scoped and **fail-if-missing**:
    a slug your key does not own (or one that expired) is `no_such_space`, never a new
    page. Best when no `space_key` was set at first publish. Mutually exclusive with
    `--space-key`.

  Both are **whole-space replace** (like re-uploading): the title, favicon, nav, and
  page set come from the publish you run — a `<path>` that no longer declares a title
  clears it, and a page dropped from the bundle 404s at its old sub-URL. Publish the
  complete space each time, not a partial diff.

  The "let a colleague / another machine open it" path.

The loopback↔hosted asymmetry is intended: loopback is live, hosted is a snapshot.

**Overrides** (flag > env > config): `--target loopback|hosted` / `$GLASSPAD_TARGET`;
`--server` / `$GLASSPAD_SERVER`; `--api-key` / `$GLASSPAD_API_KEY`; `--template`;
`--space-key` / `$GLASSPAD_SPACE_KEY`; `--update <slug>` (hosted, flag-only —
replace an existing space by its capability slug); `--title`; `--port` (loopback);
`--no-open`. The API key is never printed.

## Inspect configuration

Use `glasspad config path` to see the effective home config-file location. It is
read-only and explicitly says when no file exists. Use `glasspad config show` (or
`glasspad config show --json`) to inspect the resolved hosted server, API-key
status, target, template, space key, bind address, and favicon. Each value includes its source: `flag`, `env`, `config-file`,
or `default`. Pass `--server` / `--api-key` to `config show` only when checking the
same overrides a publish invocation would use. API-key material is always redacted.

## Authoring

**Markdown** is rendered through the space's template. For full control, author
**HTML**:

- **Fragment (default).** Write body content; glasspad wraps it in a themed skeleton
  (design tokens, correct light/dark theme, the nav bridge, opt-in base libraries):

  ```html
  <h1>Sales Q3</h1>
  <div id="chart"></div>
  <script>gp.chart('#chart', { /* vega-lite spec */ })</script>
  ```

- **Full document.** A file starting with `<!doctype html>` / `<html>` (after any
  BOM / whitespace / comments — detected tolerantly) is served **verbatim**; you own
  the whole page. Opt into in-space nav by including `/_gp/v1/bridge.js` yourself.

Base libraries live under `/_gp/v1/*` (`base.css`; `charts.js` = a thin
`gp.chart(el, spec)` over Vega-Lite). `assets/*` in a space are served by path.

## Return channel: get user input back (interactive artifacts)

An artifact can send user input **back to you** — a form answer, a button choice, a
wizard step — so an agent↔human round-trip through a rich UI works. The artifact
never gets network access; input flows `artifact → trusted shell → server → you`,
and you read it with `glasspad await-submission`.

**Author side (in a fragment artifact).** Call `gp.submit(data)` with any
JSON-serializable value, or just write an ordinary `<form>` — its submit is
intercepted and routed for you:

```html
<button type="button" onclick="gp.submit({approved: true})">Ship it</button>
<button type="button" onclick="gp.submit({approved: false})">Hold</button>
<!-- …or a plain form: -->
<form><input name="note"><button type="submit">Send</button></form>
```

`gp.submit` is available in **fragment** artifacts. A full-document artifact owns
its page; keep to fragments for forms.

**Agent side — run `await-submission` BACKGROUNDED.** It blocks on a server-side
long-poll and returns the human's answer as its result, so you fire it in the
background and get re-invoked with the answer when the user submits:

```bash
# Loopback: --port targets your local publish. Run it backgrounded.
glasspad await-submission myspace --port 3000 --timeout 120 --json
# → {"timed_out":false,"submissions":[{"id":1,"data":{"approved":true},...}],"cursor":1}

# Hosted: a --server (or $GLASSPAD_SERVER) + API key; <slug> is the page slug.
glasspad await-submission <slug> --server https://pad.example.com --json
```

- The **slug** is the space name (loopback) or the page slug (hosted).
- On a submission: stdout is one compact JSON submission per line; exit `0`.
- On **timeout**: `{"timed_out":true,"cursor":N}` and exit `3` — re-arm with
  `--since N` or give up.
- `--since <cursor>` dedupes across arms; `--timeout <secs>` bounds the hold
  (1–300, default 30). Optional `--stream` (+`--follow`) rides an SSE stream instead
  of the long-poll, with the same result shape, for sub-second / many-page cases.

**Came back later? Drain the backlog with `submissions`.** A hosted page keeps every
answer in a durable store whether or not an agent is listening, so if you published a
page and walked away, the submissions are still there. `glasspad submissions <slug>`
does a single non-blocking poll and returns the whole retained backlog at once — no
long-poll, no cursor bookkeeping:

```bash
# Hosted only: --server (or $GLASSPAD_SERVER) + API key; <slug> is the page slug.
glasspad submissions <slug> --server https://pad.example.com --json
# → {"submissions":[{"id":1,"data":{...}},{"id":2,...}],"cursor":2}
```

- `--since <cursor>` (default `0` = the whole retained backlog) skips already-seen
  ids. Owner-scoped: a slug your key does not own is an opaque `no_such_page`.
- Exit `0` whether or not the backlog is empty (an empty backlog is a valid answer,
  not an error). Submissions survive the server's retention window; `publish` prints
  both this command and the exact retention for the page you just published.

**Multi-round (re-render in place).** After a submission you can update the *same
live page* and the user's open view swaps to the new content:

- **Loopback**: just rewrite the served file — the browser reloads automatically.
- **Hosted**: `glasspad push-round <slug> <file>` (same `--server`/API key as
  publish; add `--markdown [--template …]` for markdown). Only the owning tenant may
  push. Every connected viewer's page swaps in place.

Each round stays inside the frozen null-origin sandbox, and a submission answering a
**stale** round is rejected (HTTP 409). Pattern: `await-submission` → act →
`push-round` (or rewrite the file) → `await-submission` again.

## Also available

- **`glasspad data <file>`** — parse a legacy `.csv`/`.json`/`.mbox` file to JSON
  rows on stdout (never starts a server), so you can fold that data into an HTML
  artifact you author. `--format` forces the parser; `--meta` adds inferred types.
- **`--json`** on any command → a stable envelope: results/data on stdout, errors
  `{schema_version, error:{code, message, …}}` on stderr with a non-zero exit
  (1 = your input to fix, 2 = system/IO).

## Advanced (see `--help`, not the standard flow)

- **`glasspad build <space> <out>`** — statically render a space to self-contained
  HTML files (no server, no live reload). For an offline docsite, or to inspect the
  raw wrapped HTML yourself while debugging.
- **`glasspad loopback <serve|open|stop>`** — explicit loopback-server management.
  `publish` (loopback target) already folds serve + open; reach for `loopback serve`
  only for direct control (e.g. serving the built-in fixtures, a custom port, or
  `loopback stop` to halt a running server). See `glasspad loopback --help`.
  - **`--bind <LAN-IPV4>` (LAN reach, security-sensitive, opt-in):** also serve on
    this explicit **private LAN IPv4** so other devices on the same local network can
    load the space (e.g. `glasspad loopback serve ./dir --bind 192.168.1.50`). OFF by
    default — no flag stays loopback-only. Loopback is always kept, so local tooling
    (`await-submission`/`open`/`stop`) is unaffected. The DNS-rebinding Host guard is
    NOT dropped: only that one IP (plus loopback) is accepted; every other Host — a
    rebinding attacker, a different LAN IP, a foreign port, an absolute-form/`:authority`
    mismatch — is still refused. It carries **no API key** — a trusted-LAN convenience,
    never a public bind: **hostnames are refused** (a name in the allowlist would
    reintroduce DNS rebinding), as are wildcard (`0.0.0.0`), IPv6, and public IPs (only
    RFC1918 / link-local / CGNAT ranges bind). Also settable via `bind:` in your **HOME**
    config only (a repo-local `.glasspad.yaml bind:` is ignored so a cloned repo can't
    opt you in); precedence flag > `$GLASSPAD_BIND` > home config. Traffic is plaintext
    HTTP — only use it on a network you trust. A loud startup warning names the exact
    reachable URL.

## Rules enforced on load (informative errors, no silent fixups)

- Slug/space names: lowercase `[a-z0-9-]`, start alphanumeric, ≤64 chars.
- Reserved names (`_gp`, `_c`, `assets`, `api`) and slug collisions are hard errors.
  So are symlinks, path traversal, non-UTF-8 files, and oversize files.
- Home artifact: `index` > `home` > first in nav order. Nav order comes from an
  optional per-space `glasspad.yaml` (`nav: [home, sales, detail]`), else
  lexicographic. That `glasspad.yaml` is structure only (title / theme / nav plus a
  local template path), never page content — usually absent, and distinct from the repo-root `.glasspad.yaml` that
  carries the publish `target`.
