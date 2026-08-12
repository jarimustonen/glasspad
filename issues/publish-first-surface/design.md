# Design — Publish-first CLI surface

Status: draft (co-designed with Jari 2026-08-12). Captures the agreed direction;
open decisions are flagged with **[DECIDE]**.

## Problem

The CLI and the agent-facing skill (`src/skill.md`) put heavy weight on the
**loopback** verbs — `serve`, `open`, `create`, `render`, `build` — while
`publish` / `publish-space` (hosted) read as afterthoughts. The skill literally
says *"Default to loopback `serve`; only leave the machine when the viewer is
elsewhere"* (skill.md §"Which mode to reach for"). As a result agents reflexively
`serve` + `open` on `127.0.0.1`, which only works on the machine glasspad runs on.

The intended **standard** workflow is the opposite: an agent hands glasspad a
**markdown** file (or a directory of them) and gets back a **URL** the human can
open from anywhere. The home config `~/.config/glasspad/config.yaml` already points
at a hosted server (`server: https://glasspad.maalla.dev`), but today it is treated
only as a *source of server/API-key for `publish`*, never as the signal that
**hosted publishing is the default way to operate**.

## Vision

**`publish` is THE default verb**, and its target is resolved from config. One verb,
markdown-first, the config decides where it lands.

```
glasspad publish <path>      # <path> = a .md/.html FILE or a DIRECTORY of them
```

- **Markdown is the standard input.** Hand it `.md`; rendering is automatic (the
  space model already renders `.md` via the `prose`/`dashboard` template). `.html`
  still works.
- **`publish` and `publish-space` merge into one verb.** A single file *is* a
  one-artifact space; a directory is an N-page space. `<path>` accepts either.
  (Server-side may keep both `POST /api/v1/pages` and `POST /api/v1/spaces`, or the
  client always sends a 1-page space bundle — implementation detail.)

### Target resolution (config precedence)

`target: loopback | hosted` is resolved in order, first wins:

1. **`.glasspad.yaml`** in the repo root (repo-local config). *New file — NOT the
   existing per-space `glasspad.yaml`, which stays structure-only (nav/title/theme).*
2. **`~/.config/glasspad/config.yaml`** (home config).
3. **Built-in default** — `target: loopback`. So with **no config at all**,
   `publish` still serves loopback: zero-config local just works.

Config carries the `target` plus, for `hosted`, the `server` + API key (already
present) and optionally a default `--space-key` / template.

### What `publish` does per resolved target

- **`target: hosted`** → upload the space to the server; return the `/p/<slug>/…`
  URL. Idempotent via `space_key` (re-publish updates in place).
- **`target: loopback`** → ensure a loopback live-reload server is running (spawn if
  none; reuse a running one), serve the space, and open/return the local URL. This
  folds today's `serve` + `create` + `render` + `open` into the default path.

Same verb; the config's `target` decides. The human never has to know which
mechanism ran — they get a URL.

### Surface after the reshape

**No backward compatibility** — `serve`, `create`, `render`, `open` are **removed**
as top-level verbs (their behavior is absorbed into `publish` + advanced loopback
management). Nothing is kept as a hidden alias.

Remaining verbs:

- **`publish <path>`** — the default. (covers old serve/create/render/publish/publish-space)
- **`build <space> <out>`** — **advanced.** Static, self-contained HTML output, no
  server. Skill mentions it only as *"if you want to inspect the raw HTML yourself,
  e.g. for debugging"* — not part of the standard flow.
- **Return channel** — `await-submission`, `push-round` (unchanged; already work
  hosted + loopback and compose with the unified `publish`).
- **`data <file>`** — standalone legacy CSV/JSON/mbox → JSON helper (unchanged).
- **Loopback management** — **advanced**, discoverable via `--help` only, not in the
  skill's main flow: start/stop/open a specific loopback server, port control, etc.
  **[DECIDE]** final names (keep `serve`/`open`/`stop` as the *advanced* loopback
  commands, or rename under a `glasspad loopback …` subcommand group?).

### Skill rewrite

`src/skill.md` is the agent-facing contract and is **part of this change**. It gets
rewritten around a single default: *"hand glasspad markdown, get a URL."* The mode
table and the "default to loopback" framing are removed; `build` and loopback
management appear only as brief "advanced" pointers to `--help`.

## Open decisions [DECIDE]

1. **Hosted: snapshot vs live.** Recommend **hosted = snapshot + idempotent
   re-publish** (re-run `publish` to update; `push-round` already gives live in-place
   swaps for the return channel). Live-reload stays a *loopback* property; a hosted
   `--watch` (auto re-publish on file change) is a later advanced opt-in, not in the
   first cut. — *Confirm this is the intended asymmetry.*
2. **Loopback-management verb names** (see above): keep `serve`/`open`/`stop` as
   advanced, or group under `glasspad loopback <cmd>`?
3. **`.glasspad.yaml` schema** — exact keys: `target`, `server`, `api_key`/key-file,
   default `template`, default `space_key`? And does a repo config partially override
   home (merge) or fully replace it (first-file-wins per key vs per-file)?

## Non-goals

- Server-side hosted run mode (`host-serve`) and its endpoints are unchanged.
- The artifact sandbox / security contract is untouched — every page stays a
  null-origin sandboxed iframe; `./test-security.sh` stays green. This is a CLI
  surface + config + skill change, not a host change.

## Rollout / scope notes

- Big surface reshape: `src/cli.rs`, `src/main.rs`, config resolution (new
  `.glasspad.yaml` + `target`), `src/skill.md` rewrite, tests, and new
  `./test-security.sh` cases only if the loopback-spawn path changes exposure
  (it shouldn't — same loopback guard).
- Design-first. Likely decompose into: (a) config resolution + `target` concept,
  (b) `publish` unification (file|dir, hosted|loopback dispatch), (c) verb removal +
  loopback-management demotion, (d) skill.md rewrite. (a)→(b) ordered; (c)/(d) follow.
