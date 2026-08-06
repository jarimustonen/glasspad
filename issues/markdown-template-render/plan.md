# Implementation plan — markdown + reusable-template render path

Server-side render of **markdown body + a referenced reusable template** into a
hosted artifact, plugged into the existing `wrap.rs` fragment-wrap seam. The
model is already decided (`item.md`): server-side render, payload =
`markdown + template reference`. This plan is the *how*.

## 1. The render seam (where it plugs in)

The whole feature reduces to producing an **artifact body string** and handing
it to the existing serve path — nothing about the security boundary changes:

```
markdown file ──▶ render_markdown()  ──▶ rendered HTML fragment
template ref  ──▶ resolve + read     ──▶ template string  (holds {{content}})
                          │
                          ▼  apply_template(template, rendered)
                    artifact BODY string
                          │
                          ▼  server::one_artifact_snapshot(name, body)
                    Snapshot ──▶ artifact_content handler
                          │
                          ▼  wrap::render_artifact(body, theme)   ← the seam
                    served on /{space}/_c/{slug} under headers::artifact_csp
```

The body is stored exactly like a `create`d artifact's HTML. The content route
(`artifact_host::artifact_content`) already wraps a **fragment** body into a
themed document with `base.css` linked + `bridge.js` injected, under the frozen
`artifact_csp`. So the built-in templates are authored as **fragments**
(`<article class="gp-prose">…</article>`), which means they inherit the
`--gp-*` design system (incl. the `.gp-prose` reading theme) and the bridge for
free, with no new wiring in `wrap.rs`/`shell.rs`.

## 2. Why the security boundary stays intact (the core argument)

The template is **client-shipped, untrusted** content. It never widens the
boundary because of *where* its output lands, structurally:

- The render output becomes the **artifact body** on `/{space}/_c/{slug}`. That
  response's CSP (`headers::artifact_csp`), sandbox, Trusted-Types, `nosniff`,
  `no-referrer`, and `Cache-Control: no-store` are all set **server-side in the
  handler**, on every response, independent of body content. A template cannot
  change a response header. A `<meta http-equiv="Content-Security-Policy">` in a
  template can only *tighten* (intersect), never widen — so it **fails closed**.
- The template output is served in the **null-origin sandbox**. It is already
  untrusted script inside the sandbox — exactly like any `create`d artifact —
  so splicing arbitrary template + arbitrary markdown-HTML adds **no new trust
  boundary** (design.md §7: the sandbox/CSP is the boundary, not sanitization).
- The **trusted parent shell** (`shell.rs`) is a *different route*
  (`/{space}/{slug}`), rendered from the server-resolved nav table. Template
  bytes never reach it. The one artifact-derived value the shell consumes — the
  title (`space::resolve_title`) — is already inserted as `textContent` /
  JSON-for-script (existing hardened path + tests). So even a markdown body that
  emits a hostile `<h1>`/`<title>` renders inert in the chrome.
- `wrap.rs` and `shell.rs` are **not modified**. The template governs only the
  body; glasspad keeps sole control of CSP / TT / nav / sandbox.

The `.gp-prose` hardening from `prose-theme` (wide-table scroll, long-URL wrap,
loose-list gaps, task-list checkboxes, `> :first/:last-child` flush) is
**honored** by emitting rendered blocks as **direct children of `.gp-prose`**
(the built-in prose template wraps `{{content}}` directly), which is the render
contract those CSS rules assume.

## 3. Module layout

- **`src/artifact_host/render.rs`** (new) — the single server-side renderer:
  - `render_markdown(md: &str) -> String` — CommonMark + GFM (tables,
    strikethrough, task lists, footnotes) via `pulldown-cmark`. Raw inline/block
    HTML passes through (default) — acceptable inside the sandbox, and the reason
    the `.gp-prose` CSS was hardened against arbitrary markup.
  - `BUILTIN_TEMPLATES: prose | dashboard` — `prose` =
    `<article class="gp-prose">{{content}}</article>` (default; the reading
    theme). `dashboard` = `<div class="gp-card">{{content}}</div>` (default
    dashboard look in a card surface). Both are fragments → auto base.css+bridge.
  - `apply_template(template, rendered) -> Result<String, TemplateError>` —
    splices the rendered HTML at the single `{{content}}` placeholder.
  - `PLACEHOLDER` contract: exactly **one** `{{content}}` (whitespace-tolerant:
    `{{ content }}` accepted). Zero → `MissingPlaceholder`; more than one →
    `DuplicatePlaceholder`. Other `{{…}}` tokens are left verbatim.
  - `TemplateError` → informative CLI errors (`invalid_template`).
- **`src/cli.rs`** — `render(...)` command + helpers: read+validate the markdown
  file (mirrors `load_single_file`'s strict checks), resolve the template ref,
  read a file template (strict, capped), render, splice, serve live.
- **`src/server.rs`** — `spawn_render_watcher(...)`: re-render on a change to the
  **markdown file** *or* (for a file template) the **template file**; swap the
  atomic snapshot + fire SSE reload; keep last-good on a transient error (mirrors
  `spawn_file_watcher`). Reuses `one_artifact_snapshot`.
- **`src/main.rs`** — the `Render` subcommand (clap).
- **`Cargo.toml`** — add `pulldown-cmark` (`default-features = false`, pure Rust,
  no network, no `unsafe` in our use).

## 4. CLI surface (per AGENTS-AI-FIRST-CLI.md)

```
glasspad render <markdown-file> [--template <ref>] [--name <space>] [-p/--port <n>]
```

- **`<markdown-file>`** — positional path. Strict validation (fail-fast §1): a
  missing path, a directory, a non-regular / oversize (> `MAX_FILE_BYTES`) /
  non-UTF-8 file each exits with a structured envelope, never a silent fixup.
- **`--template <ref>`** — default `prose`. **Resolution rule (deterministic,
  documented):** if `ref` is exactly a built-in name (`prose` | `dashboard`) →
  built-in; **otherwise** → a filesystem path to a template file (read,
  size-capped, UTF-8, must contain exactly one `{{content}}`). Built-in names
  contain no `/`/`.`, so a local file named `prose` is reachable as `./prose`
  (which ≠ `"prose"` → path). Unambiguous.
- **`--name <space>`** — default the markdown file stem; validated against the
  shared space grammar (`artifact_host::valid_space`), same as `create`.
- **`-p/--port`** — `u16`, range `1..`, default 3000 (matches the other cmds).
- **No interactive prompts.** All errors: structured `{schema_version, error:
  {code, message, invalid_value?}}` on **stderr**, exit 1 (user) / 2 (system),
  via the shared `exit_error`.
- **`--json` startup envelope** (stdout, mirrors `create` + template fields):
  `{schema_version, serving:true, port, space, slug:"index", home:"index", url,
   template, template_kind:"builtin"|"file", warnings:[]}`.

New error `code`s: `invalid_template` (placeholder missing/duplicated),
`template_not_found` (template file missing), plus reuse of `no_such_path`,
`not_a_file`, `file_too_large`, `not_utf8`, `invalid_space_name`, `io_error`.

## 5. Tests

Unit (`render.rs`):
- markdown → HTML (headings, GFM table, task list, code fence, strikethrough).
- `apply_template`: single placeholder spliced; whitespace variant; missing →
  err; duplicate → err; unrelated `{{x}}` left verbatim.
- built-in `prose` puts rendered blocks as **direct children** of `.gp-prose`
  (the render contract the hardened CSS depends on).

Integration (`artifact_host` / `cli`):
- a rendered fragment served on `_c` is **wrapped** (base.css + bridge, doctype).

**Adversarial** (the mandated fail-closed cases):
- a template/markdown emitting `<script>`, `<meta http-equiv=CSP>`, or a stray
  `</body></html>` still yields a `_c` response carrying the **unchanged frozen
  `artifact_csp`** (sandbox + `connect-src 'none'` + `'unsafe-eval'`), i.e. the
  template cannot widen CSP — proven at the header level.
- a markdown body emitting a hostile `<h1>`/`<title>` renders **inert** in the
  trusted shell (title flows through the existing `textContent`/JSON-for-script
  path; assert no raw executable markup in the shell document).
- `./test-security.sh` (41 checks + Wave 2a) stays green — the render path adds
  no host/CSP/bridge/shell surface, so no probe changes; run it as the gate.

## 6. Green gate (all before merge)

`cargo fmt --all --check` · `cargo clippy --all-targets -- -D warnings` ·
`cargo test` · `./test-security.sh`. Then `/llm-review` + `/assess-findings`,
apply mechanical/confirmed fixes, self-merge if green and no user decision.

## 7. Non-goals (scope guard)

- No client-side render / shipping final HTML (rejected in `item.md`).
- No changes to `wrap.rs` / `shell.rs` / CSP / headers / sandbox.
- No template *templating language* beyond the single `{{content}}` slot (a
  template is plain HTML with one insertion point — reusable, not a DSL).
- Syntax-highlight tokens (`--gp-syntax-*`) and footnote-class vocabulary remain
  deferred (noted by `prose-theme`); the renderer emits standard GFM classes.
