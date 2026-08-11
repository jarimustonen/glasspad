---
name: glasspad
description: Show rich visual HTML views (dashboards, charts, interactive UIs) to the user in their browser. Use when asked to visualize, plot, chart, dashboard, or "show me" something.
cli_version: "0.2.0"
schema_version: 1
---

# Glasspad — Visual HTML Output

Render your own HTML as a live, safely-sandboxed page the user views in their
browser. You author plain HTML; Glasspad serves it on loopback and reloads the
browser when you edit the file. Every command takes paths as arguments, emits a
stable `--json` envelope, and fails with an informative error (never a prompt).

## The model

- A **space** is a directory of artifacts sharing a URL namespace: `/{space}/`.
- An **artifact** is one HTML view, addressed by a **slug** = its filename stem
  (`sales.html` → slug `sales`). You link between them with ordinary relative
  links (`<a href="./detail">`).
- Serving is live: edit a file on disk and the browser reloads. The directory is
  the single source of truth — there is no upload/push step.

## Which mode to reach for

Pick by **who needs to see it and how it travels** — the authoring model (HTML or
markdown, spaces, slugs) is the same underneath. Default to loopback `serve`; only
leave the machine when the viewer is elsewhere.

| You want to… | Use | Notes |
|---|---|---|
| Show the user on **this machine** while you work | `glasspad serve ./dir` (or `create <file>` for a single file) | Loopback `127.0.0.1`, keeps the DNS-rebinding Host guard, live reload. The private on-your-machine view — "show me while I work." |
| Same, but the payload is **markdown** and you want a themed page | `glasspad render <file.md> [--template prose\|dashboard\|./tpl.html]` | Server-side md→HTML spliced into the template's `{{content}}` slot; the template governs the body only (sandbox/CSP stay glasspad's). Still loopback + live reload. |
| Let a **colleague / another machine** open it over the network | `glasspad publish <file>` → hosted share server | API-key ingest; returns a public capability-slug URL (`/p/<slug>`, `noindex` — "hold the link"). `--markdown` renders md server-side; `--title`, `--no-open`; `--idempotency-key <k>` makes a repeat publish return the first page (HTTP 200) instead of a new one — exactly-once for a deterministic caller. Server + key from `--server`/`--api-key`, `$GLASSPAD_SERVER`/`$GLASSPAD_API_KEY`, or `~/.config/glasspad/config.yaml`. |
| Preview on an **external seat** (not this box) | external seat preview | The external transport path — hands the rendered page to a remote seat you reach over that transport, rather than the local browser or the share server. |
| **No server at all** — static, self-contained files (offline / docsite) | `glasspad build <space> <out>` | Renders a space to a self-contained static bundle in `<out>`; no bind, no live reload. The "just ship the files" option. |

Operator note: the hosted share server is a separate run mode —
`glasspad host-serve --bind … --public-host … --api-key-file … --store …` (public
bind, no loopback guard). Agents `publish` **to** it; they don't run it.

## Authoring: write HTML

**Fragment (default).** Write body content; Glasspad wraps it in a themed
skeleton (design tokens, correct light/dark theme, the nav bridge, opt-in base
libraries):

```html
<h1>Sales Q3</h1>
<div id="chart"></div>
<script>gp.chart('#chart', { /* vega-lite spec */ })</script>
```

**Full document.** If the file starts with `<!doctype html>` or `<html>` (after
any BOM / whitespace / leading comments — detected tolerantly, not by a naive
prefix), it is served **verbatim**; you own the whole page. Opt into in-space nav
by including `/_gp/v1/bridge.js` yourself.

Base libraries live under `/_gp/v1/*` (e.g. `base.css`, `charts.js` = a thin
`gp.chart(el, spec)` over Vega-Lite). `assets/*` in a space are served by path.

## Commands

```bash
glasspad serve ./myspace          # serve a directory live (the primary loop)
glasspad create ./report.html     # one-artifact space from a single file
glasspad render ./notes.md        # render markdown via a template, serve it live
glasspad build ./myspace ./out    # statically render a space to HTML files (no server)
glasspad open myspace             # open http://127.0.0.1:3000/myspace/ in the browser
glasspad data ./old.csv           # optional: parse a legacy CSV/JSON/mbox file to JSON rows
```

- `render <file.md>` renders markdown → HTML server-side and hosts it. Choose the
  look with `--template`: a built-in (`prose`, the default reading theme; or
  `dashboard`, the card look) or a path to your own template HTML file that
  contains one `{{content}}` slot (e.g. `--template ./layout.html`). The template
  styles only the artifact body — the sandbox/CSP stay glasspad's. Re-renders on
  save (editing the markdown, or a file template, reloads the browser).
- `build <space> <out>` statically renders a space to self-contained HTML files —
  no server, no bind. Each artifact becomes `<slug>.html` (wrapped exactly as the
  live host would serve it); the home is `index.html` (a redirect when the home is
  not literally `index`). Default **self-contained**: the base libs are bundled
  under `_gp/v1/` and referenced relatively, so the output works offline (open
  `index.html`, or serve the dir at web root). `--shared-libs` references the libs
  at the absolute `/_gp/v1/…` path and skips bundling (smaller; needs a host that
  serves them). `--force` writes into a non-empty dir; `--dry-run` plans without
  writing. For an offline docsite / preview transport, not a live-reload loop.
- `serve`/`create`/`render` run until killed — start them in the background, then `open`.
- `data <file>` is a standalone helper (never starts a server): it parses a
  legacy `.csv` / `.json` / `.mbox` file and prints the rows as JSON on stdout,
  so you can fold that data into an HTML artifact you author. `--format` forces
  the parser; `--meta` also emits inferred per-field types.
- Add `--json` to any command for a stable envelope. `serve`/`create` print a
  startup line `{schema_version, serving, port, space, url, ...}` to stdout;
  errors print `{schema_version, error:{code, message, ...}}` to stderr with a
  non-zero exit (1 = your input to fix, 2 = system/IO).
- `--port N` (default 3000). `create --name <space>` / `render --name <space>`
  override the space name (default: the file stem, which must be a valid name).
  `open --no-browser` prints just the URL.

## Typical flow

```bash
# One file:
glasspad create ./report.html --json    # → {"url":"http://127.0.0.1:3000/report/", ...}
glasspad open report

# A directory of linked pages (index.html = home):
glasspad serve ./dashboard --json &
glasspad open dashboard
# ...edit files; the browser reloads on save.
```

## Return channel: get user input back (interactive artifacts)

An artifact can send user input **back to you** — a form answer, a button choice,
a wizard step — so an agent↔human round-trip through a rich UI works. The artifact
never gets network access; input flows `artifact → trusted shell → server → you`,
and you read it with `glasspad await-submission`.

**Author side (in a fragment artifact).** Call `gp.submit(data)` with any
JSON-serializable value, or just write an ordinary `<form>` — clicking its submit
button is intercepted and routed for you:

```html
<h1>Approve the deploy?</h1>
<button type="button" onclick="gp.submit({approved: true})">Ship it</button>
<button type="button" onclick="gp.submit({approved: false})">Hold</button>

<!-- …or a plain form: -->
<form><input name="note"><button type="submit">Send</button></form>
```

`gp.submit` is available in **fragment** artifacts (they get the bridge). A
full-document artifact owns its page; keep to fragments for forms.

**Agent side — run `await-submission` BACKGROUNDED.** It blocks on a server-side
long-poll and returns the human's answer as its result, so you fire it in the
background and get re-invoked with the answer when the user submits — no polling
loop:

```bash
# Loopback: --port targets your local `serve`/`create`. Run it backgrounded.
glasspad await-submission myspace --port 3000 --timeout 120 --json
# → {"timed_out":false,"submissions":[{"id":1,"data":{"approved":true},...}],"cursor":1}

# Hosted: a --server (or $GLASSPAD_SERVER) + API key; <slug> is the page slug.
glasspad await-submission <slug> --server https://pad.example.com --json
```

- The **slug** is the space name (loopback) or the page slug (hosted).
- On a submission: stdout is one compact JSON submission per line; exit `0`.
- On **timeout**: a distinct result `{"timed_out":true,"cursor":N}` and exit `3`
  — re-arm with `--since N` (to skip what you already saw) or give up.
- `--since <cursor>` only returns submissions after that id (dedupe across arms);
  `--timeout <secs>` bounds the hold (1–300, default 30).

The typical loop: `publish`/`serve` an interactive page → `await-submission`
backgrounded → act on the returned `data` → (optionally) re-render the next step.

**Optional SSE transport (`--stream`).** For sub-second streaming or watching many
pages, add `--stream`: the command holds a server-push `EventSource`
(`…/submissions/stream`) instead of the long-poll and returns the first submission
(exit `0`) or times out (exit `3`) with the **same** result shape — so it is a drop-in
for the default. Add `--follow` to keep the stream open and print every submission as
it lands (until `--timeout`). The plain long-poll stays the default/fallback; reach for
`--stream` only when the latency or many-pages case calls for it.

**Multi-round (re-render in place).** After a submission you can update the *same
live page* and the user's open view swaps to the new content — a conversational UI
in one page, no new URL:

- **Loopback** (`serve`/`create`): just rewrite the served file. The browser
  reloads to the new round automatically; the next submission binds to it.
- **Hosted**: `glasspad push-round <slug> <file>` (same `--server`/API key as
  `publish`; add `--markdown [--template …]` for markdown). Only the page's owning
  tenant may push. It prints `{round, content_version}`; every connected viewer's
  page swaps to the new round in place.

Each round stays inside the frozen null-origin sandbox (no network, no `allow-forms`),
and a submission that answers a **stale** round is rejected (HTTP 409
`content_version_mismatch`) — so a late click on an old round can't be mistaken for
an answer to the current one. Pattern: `await-submission` → act → `push-round` (or
rewrite the file) → `await-submission` again.

## Rules enforced on load (informative errors, no silent fixups)

- Slug/space names: lowercase `[a-z0-9-]`, start alphanumeric, ≤64 chars.
- Reserved names (`_gp`, `_c`, `assets`, `api`) and slug collisions are hard
  errors. So are symlinks, path traversal, non-UTF-8 files, and oversize files.
- Home artifact: `index.html` > `home.html` > first in nav order. Nav order comes
  from an optional `glasspad.yaml` (`nav: [home, sales, detail]`), else
  lexicographic. `glasspad.yaml` is structure only (title / theme / nav), never
  content — usually absent.
