# TODO — Glasspad handoff

Orchestrator entry point for `/stint`. This is the round-by-round plan; the
authoritative detail lives in the issue tracker (`issuectl`), not here.

## Where we are

**The v0.2 rewrite is COMPLETE.** Glasspad is a CLI tool + lightweight
HTML-artifact host: the calling agent authors **HTML directly** in a directory,
`glasspad serve`s it, and it renders in a null-origin sandboxed iframe with nav
chrome + base libraries. The old ~6000-line section-DSL path is gone.

- **Scope: localhost-only.** No team/cloud, accounts, or persistence backends.
  **No MCP** — glasspad is a CLI tool (`serve`/`create`/`open`/`data`/`skill`).
- **Green baseline on `main`:** `./test-security.sh` = 41 browser checks +
  Wave 2a probes; `cargo build`/`clippy --all-targets`/`test` all clean.

**Last stint (2026-07-25) cleared the deck for release:**
- Closed the moot decision issues `structured-api-errors` + `auth-status-codes`
  (both referenced the removed `/api/pads` surface).
- Closed the `mcp-integration` epic as **obsolete** — PO decision: no MCP.
- Fixed + shipped `skill-install-json`: `glasspad skill --install-claude --json`
  now emits a proper versioned envelope (success + structured error), plain
  output unchanged. `/llm-review` findings applied; merged green.

## ▶ Start here (next session) — RELEASE IS THE #1 TASK

**The next and most important work is releasing glasspad 0.2.0 as a proper
open-source project.** Tracked as epic **`release-oss`**
(`issuectl show release-oss`, priority high). PO decision (2026-07-25): publish
across **all three** channels — git tag + `cargo install --git`, **crates.io**,
and a **Homebrew tap + GitHub release binaries** (issuectl/orchestratectl style).

### ⛔ HARD GATE — check the `/oss-*` skills FIRST, before any release work

The release is meant to be driven by a **new `/oss-*` skill family**
(`/oss-release`, `/oss-…`) built for exactly this kind of open-source-release
task. **These skills do not exist yet.**

So the very first action of the next stint is:

1. **Check whether the `/oss-*` skills are installed** (look for them in the
   available-skills list / `.claude/skills/`).
2. **If they are NOT present → STOP.** Do not start the release, do not spawn
   worktrees, do not improvise the release by hand. **Warn the user** that the
   `/oss-*` skills are not in place yet and that we agreed not to proceed until
   they are. End the stint there.
3. **If they ARE present → use them** to drive the `release-oss` epic.

In short: **the stint runs, but release work proceeds only after the `/oss-*`
skills are in place.** No skills → halt + warn, nothing else.

### Release scope (see `release-oss` for full detail)

- **Must:** reconcile version → **0.2.0** (`Cargo.toml` 0.1.0 vs `skill.md`
  0.2.0 — the `--json` envelopes now surface `cli_version`); add **LICENSE**;
  crates.io **package metadata** in `Cargo.toml`; **CHANGELOG.md**; green gate
  (build/clippy/test/`test-security.sh`); **install verification**
  (`cargo install --path .` / `--git`).
- **OSS hygiene:** CONTRIBUTING, issue/PR templates, README badges, **CI**
  (GitHub Actions), **release automation** (tag → binaries → GH release + brew +
  crates.io).
- **Decide at stint start:** fold the process-management gaps
  (`glasspad stop`, `GLASSPAD_PORT`, PID file) into 0.2.0, or defer to 0.2.1
  (default lean: defer, ship the current green surface).

## Backlog

- `release-oss` (epic, high) — **the forward work; see the gate above.**
- No other open work items. (v0.1 section-DSL issues were closed `obsolete`;
  `mcp-integration`, `finalization-release`, `structured-api-errors`,
  `auth-status-codes` all closed.)

## Verify / deploy (localhost)

Per `CLAUDE.md`: after editing host code or a base lib, `cargo build`, restart
`glasspad serve`, reload the space. `./test-security.sh` (41 browser checks +
Wave 2a probes) is the regression gate after any host/header/CSP/bridge change.
Use `./test-browser.sh` (check `./test-browser.sh errors` first) for ad-hoc
browser automation.

## Notes for the orchestrator

- Keep `main` clean — commit issue/status changes immediately (`CLAUDE.md`).
- 43 v0.1 section-DSL issues were closed `obsolete`; don't resurrect them.
- Track all planning under the issue, not as loose files.
