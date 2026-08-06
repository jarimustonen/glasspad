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
| Let a **colleague / another machine** open it over the network | `glasspad publish <file>` → hosted share server | API-key ingest; returns a public capability-slug URL (`/p/<slug>`, `noindex` — "hold the link"). `--markdown` renders md server-side; `--title`, `--no-open`. Server + key from `--server`/`--api-key`, `$GLASSPAD_SERVER`/`$GLASSPAD_API_KEY`, or `~/.config/glasspad/config.yaml`. |
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
glasspad open myspace             # open http://127.0.0.1:3000/myspace/ in the browser
glasspad data ./old.csv           # optional: parse a legacy CSV/JSON/mbox file to JSON rows
```

- `render <file.md>` renders markdown → HTML server-side and hosts it. Choose the
  look with `--template`: a built-in (`prose`, the default reading theme; or
  `dashboard`, the card look) or a path to your own template HTML file that
  contains one `{{content}}` slot (e.g. `--template ./layout.html`). The template
  styles only the artifact body — the sandbox/CSP stay glasspad's. Re-renders on
  save (editing the markdown, or a file template, reloads the browser).
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

## Rules enforced on load (informative errors, no silent fixups)

- Slug/space names: lowercase `[a-z0-9-]`, start alphanumeric, ≤64 chars.
- Reserved names (`_gp`, `_c`, `assets`, `api`) and slug collisions are hard
  errors. So are symlinks, path traversal, non-UTF-8 files, and oversize files.
- Home artifact: `index.html` > `home.html` > first in nav order. Nav order comes
  from an optional `glasspad.yaml` (`nav: [home, sales, detail]`), else
  lexicographic. `glasspad.yaml` is structure only (title / theme / nav), never
  content — usually absent.
