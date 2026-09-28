---
name: glasspad
description: Show rich visual HTML views (dashboards, charts, interactive UIs) to the user in their browser. Use when asked to visualize, plot, chart, dashboard, or "show me" something.
cli_version: "0.18.5"
schema_version: 1
---

# Glasspad

Glasspad turns a Markdown or HTML file, or a directory of them, into a page the
user opens in a browser. You author the content; `glasspad publish <path>` returns
the URL. A page can also carry the user's answer back to you, so a form or a row of
buttons becomes a way to ask something richer than a chat message allows.

```bash
glasspad publish ./report.md      # one file → a one-page space
glasspad publish ./dashboard/     # a directory → a multi-page space
```

Every command validates its input strictly, takes `--json` for a stable envelope,
and fails with a structured error instead of prompting. Exit 1 means your input
needs fixing, 2 a system or I/O problem. `glasspad <command> --help` documents the
flags. This file covers what the help cannot: the model, the sandbox you are
authoring into, and the traps.

## Where the URL lives

`publish` has no "where" argument. It reads a `target` from config, which `--target`
overrides for one run: `loopback` serves on this machine and is the default when no
config exists; `hosted` uploads to a share server and returns a link others can
open. Config merges per key, and the first file that sets a key wins: a
`.glasspad.yaml` found by walking up from the working directory, then the home
config. `glasspad config path` prints the home file's effective location and
`glasspad config show` the resolved values with their provenance. Flags and
`GLASSPAD_*` environment variables override both. A repo can therefore pin
`target: hosted` and `server:` while the API key stays in the home config.

The two targets behave differently on purpose.

**Loopback is live.** `publish` binds `127.0.0.1`, opens the browser, watches the
files, and reloads the page whenever you edit them. It announces the URL as soon as
it binds and then keeps running until killed, so start it in the background. To
change what the user sees, edit the file. `glasspad loopback stop` halts the server.
Nothing leaves the machine; this is the private "show me while I work" view.

**Hosted is a snapshot.** `publish` uploads the space and prints a `/p/<slug>/` URL
whose slug is an unguessable capability, served `noindex`: anyone holding the link
can read it, nobody else finds it. Running the same `publish <path>` again updates
the same URL in place. Identity derives locally from the source's canonical path
(the path itself is not sent), or from a `space_key` set in config or with
`--space-key` when the identity should survive moving the source or changing
machines. `--new` deliberately mints a fresh URL. `--update <slug>` retargets a URL
you already hold and refuses rather than creates if your key does not own it. Each
hosted publish replaces the whole space: title, favicon, nav, and page set come from
this publish, so a page you drop 404s at its old address and a title you stop
declaring disappears. Publish the complete space, not a diff.

Before a hosted publish, notice what is in the content. Loopback shows the user
their own data; hosted puts it on a server behind a link that may be forwarded. The
configured target is usually the user's answer to that question already. Sensitive
material heading for a hosted target is the one case worth a sentence to them first.

**Credentials.** `api_key` in config accepts `{ env: VAR }` or `{ file: path }`
(relative to the config file's directory) as well as an inline value; the
indirections keep the secret out of a file that might be committed. Keep the key in
the home config, not the repo. When a repo's `.glasspad.yaml` sets `server:` while
the key comes from your home config, `publish` warns, because a cloned repository
could otherwise redirect your credential to a server of its choosing; passing
`--server` explicitly, or moving `server` into the home config, is how you say you
meant it (`--api-key` alone does not silence the warning). The key is never
printed, and the follow-up commands `publish` suggests omit it so they can be pasted
without putting a secret on argv or into shell history.

## Spaces, pages, and the two YAML files

A space is a URL namespace holding one or more pages. A page's slug is its filename
stem (`sales.md` → `sales`): lowercase `[a-z0-9-]`, starting alphanumeric, at most
64 characters, and not one of the reserved names `_gp`, `_c`, `assets`, `api`. Two
files mapping to one slug (`sales.md` beside `sales.html`) are a hard error, never
silently resolved. A top-level `AGENTS.md` or `CLAUDE.md` is skipped, not published.
Pages link to each other with ordinary relative links. Files under `assets/` are
served by path. The home page is `index`, else `home`; a space with neither and two
or more pages (or grouped nav) gets a generated table-of-contents landing at `index`,
and a lone page is its own home.

An optional `glasspad.yaml` inside the space describes structure only: `title`,
`nav` (an ordered list of slugs), `groups` (a labelled sidebar with one level of
nesting, for docsite-sized spaces), and `template`. It is usually absent. The
repo-root `.glasspad.yaml` is a different file with a different job: `target`,
`server`, `api_key`, a default `template`, `space_key`, and `favicon`. `bind` is
honoured from the home config only, for the reason given under LAN reach below.

The scanner rejects symlinks, paths that resolve outside the space, non-UTF-8 files,
files over 8 MiB, and spaces over 64 MiB. Each of those is either an escape route or
a resource-exhaustion vector for a host that serves untrusted authored content.
Errors name the file and the rule.

## Authoring into the sandbox

Every page renders inside a null-origin sandboxed iframe under a strict Content
Security Policy, with Glasspad's trusted shell around it for navigation and theme.
Your HTML controls the document inside the iframe, never the browser tab. The
policy shapes what you can write:

- The artifact has no network. `connect-src 'none'` blocks `fetch`, beacons, and
  websockets. Scripts, styles, and fonts load only from Glasspad's own host; images
  from the host or `data:` URLs. Inline everything else: data, images, styles. A Vega
  spec with `data.url` fails at load.
- Inline `<script>` and `'unsafe-eval'` are allowed, so the page can compute.
- Native form submission is off. The bridge intercepts it (next section), so a plain
  `<form>` still works, as a return channel rather than a navigation.

**Markdown** is the standard input. It renders through a template: `prose` (default,
for reading), `dashboard` (cards), `report`, `board`, `index` (a linked directory),
`table` (a data table), or a relative path to your own fragment template with one
`{{content}}` slot. A custom template applies to every Markdown page and is rendered
into the uploaded bodies, so hosted spaces stay self-contained. Raw HTML in Markdown
passes through unsanitized, so a chart or a form can live in a `.md` page. Glasspad
does not infer document semantics; glossary links and cross-references belong in the
producer's build step (`docs/markdown-preprocessing.md` in the repository).

**HTML fragments** are wrapped in a themed skeleton with `base.css` (the `--gp-*`
design tokens, which follow the user's light or dark theme) and `bridge.js`
(navigation and the return channel) injected. The chart helper is not injected;
include it yourself:

```html
<h1>Sales Q3</h1>
<div id="chart"></div>
<script src="/_gp/v1/charts.js"></script>
<script>
  gp.chart('#chart', {
    mark: 'bar',
    data: { values: [ /* inline rows */ ] },
    encoding: { /* … */ }
  });
</script>
```

`gp.chart(el, vegaLiteSpec)` renders through Vega-Lite, reads the `--gp-*` tokens
for its theme, and re-renders when the theme flips.

**Full HTML documents** (first markup `<!doctype html>` or `<html>`, detected after
any BOM, whitespace, or comments) are served verbatim inside the iframe. You get the
whole document and none of the injection: no tokens, no navigation, no `gp.submit`.
Loading `/_gp/v1/base.css` and `/_gp/v1/bridge.js` yourself restores most of it, but
fragments are the tested path, especially for forms, so prefer a fragment unless you
need to control the document.

## Getting an answer back

In the page, call `gp.submit(data)` with any JSON-serializable value, or write an
ordinary `<form>`; the bridge serializes it on submit and routes it through the
trusted shell to the server. The page never gains network access.

```html
<button onclick="gp.submit({approved: true})">Ship it</button>
<form><input name="note"><button type="submit">Send</button></form>
```

On your side, `glasspad await-submission <slug>` long-polls the server and returns
when the user submits. It blocks, so run it in the background and act when it comes
back. The slug is the space name on loopback (the directory name or file stem; pass
`--port` to reach your local server) and the page slug on hosted. A timeout exits 3
and, under `--json`, returns `{"timed_out":true,"cursor":N}`; re-arm with `--since N`
so you do not see the same submission twice. `--stream --follow` rides an SSE stream
instead, for many pages or sub-second latency.

A hosted page keeps every submission for the server's retention window whether or
not anyone is listening. If you published, left, and came back, `glasspad
submissions <slug>` returns the whole backlog in one non-blocking call and exits 0
even when empty. `publish` prints that command, and the retention period when the
server reports one. Loopback has no durable store; it is a live session.

**Multiple rounds.** After a submission you can replace the page the user is looking
at. On loopback, rewrite the file. On hosted, `glasspad push-round <slug> <file>`
(with `--markdown` for a Markdown body) swaps the content for every connected viewer.
Each round carries a content version, and a submission from a stale round is rejected
with HTTP 409, so an answer to the old question cannot pass as an answer to the new
one.

## Other tools

`glasspad data <file>` parses a legacy CSV, JSON, or mbox file to JSON rows on
stdout so you can inline the data into a page; it never starts a server.
`glasspad build <space> <out>` renders a space to self-contained static HTML, for an
offline docsite or to inspect what the wrapper produced. `glasspad loopback
serve|open|stop` gives explicit control of the loopback server when `publish` folds
too much together, for example to serve the built-in fixtures.

**LAN reach.** `loopback serve --bind <LAN-IPv4>` additionally serves on one private
address so another device on the same network can open the space. It is off by
default and accepts only a literal RFC1918, link-local, or CGNAT IPv4 address: a
hostname would reintroduce the DNS-rebinding attack the Host guard exists to stop,
and a public or wildcard address would expose a server that has no authentication.
Traffic is plaintext HTTP. Only the home config may set `bind`, so a cloned repository
cannot opt the machine onto its LAN. It is a trusted-network convenience and the
startup warning names the reachable URL.

`glasspad doctor` checks the configuration and that this skill's metadata matches the
installed binary. `glasspad skill list` and `glasspad skill print glasspad` expose the
bundled skill; bare `glasspad skill` prints its raw text for older callers. `glasspad
skill install` writes that same text to the selected agent's skill directory. For a
project, `--agent pi` writes `.pi/skills/glasspad/SKILL.md`; `--agent claude` writes
`.claude/skills/glasspad/SKILL.md` (and requires `.claude/` to exist). With `--user`,
the destinations are `~/.pi/agent/skills/glasspad/SKILL.md` and
`~/.claude/skills/glasspad/SKILL.md`. The default `--agent all` writes both. The
older `skill --install-claude` spelling still uses this installer. Reinstall after
upgrading the CLI when the installed skill drifts from the binary.
