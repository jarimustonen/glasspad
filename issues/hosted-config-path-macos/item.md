---
created: 2026-08-09
updated: 2026-08-09
type: bug
status: fixed
priority: normal
commits:
- hash: 755de91
  summary: honor $XDG_CONFIG_HOME/~/.config on all platforms, legacy dirs::config_dir() fallback
- hash: c1f9b4e
  summary: surface unreadable/malformed config, don't leak relative XDG (llm-review fixes)
- hash: 767768c
  summary: record resolution + rationale in issue
closed: 2026-08-09
---

# publish config path: --help says ~/.config but macOS resolves to ~/Library/Application Support

_Source: src/cli.rs load_publish_config_

## Description

`glasspad publish` resolves its config file via `dirs::config_dir()` (src/cli.rs ~1906), which on macOS is `~/Library/Application Support/glasspad/config.yaml` — but the `publish --help` text and the doc comment at src/cli.rs:1676,1711,1722 (and skill.md) all state `~/.config/glasspad/config.yaml` unconditionally. On a macOS host a user who follows the help puts config.yaml under ~/.config, the binary looks under ~/Library/Application Support, finds nothing, and publish fails with `missing_server` even though the file exists.

Observed 2026-08-09 while deploying share.example.com: config at ~/.config/glasspad/config.yaml was ignored on macOS (gertrud); moving it to ~/Library/Application Support/glasspad/config.yaml fixed it.

Fix options (pick one, cross-platform-consistent):
1. Honor $XDG_CONFIG_HOME and ~/.config on all platforms (many CLIs do this; matches the help text and keeps one documented path), OR
2. Keep dirs::config_dir() but make the --help text + doc comments + skill.md platform-accurate (show the macOS path).

Option 1 gives users one path to remember and matches every doc string already shipped; option 2 is the smaller change. Either is fine — the bug is that docs and behavior disagree on macOS.

## Resolution (2026-08-09)

**Chose Option 1** — honor `$XDG_CONFIG_HOME` / `~/.config` on all platforms.
Rationale: the docs (`--help`, doc comments, `src/skill.md`) already documented
`~/.config/glasspad/config.yaml` unconditionally, so aligning behavior to one
cross-platform documented path (rather than making every doc string
platform-branch) is the smaller net change to the shipped user contract and
gives users a single path to remember. It was not surprisingly invasive.

**Resolution order** (`load_publish_config`, first existing candidate wins):
1. `$XDG_CONFIG_HOME/glasspad/config.yaml` if `$XDG_CONFIG_HOME` is set,
   non-empty, and absolute (empty/relative treated as unset per XDG spec);
   else `~/.config/glasspad/config.yaml`.
2. Backward-compat fallback: the platform `dirs::config_dir()` location
   (macOS `~/Library/Application Support/glasspad/config.yaml`), deduped
   against #1 and only if absolute — so existing macOS users don't break.

Docs already matched #1; extended the `PublishConfig` doc comment to spell out
the fallback precedence. `skill.md` / `main.rs` `--help` keep `~/.config` as the
primary path (now accurate on macOS + Linux).

**Post-review hardening** (`/llm-review`, 4-model consensus): an existing-but-
unreadable config (permissions, a directory, non-UTF-8) is now a hard
`unreadable_config` error instead of silently falling through to a legacy file
with a *different* server/api_key; the legacy fallback path is filtered for
absoluteness so a relative `$XDG_CONFIG_HOME` can't leak in via
`dirs::config_dir()` and be read against the process CWD.

Kept first-existing-wins (not field-merging across the two files) and kept the
XDG-first behavior cross-platform incl. Windows (deliberate, per Option 1);
Windows users' existing `%APPDATA%` config still works via the fallback. CI is
`ubuntu-latest` only, so Unix-style test path fixtures are fine.
