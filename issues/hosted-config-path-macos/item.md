---
created: 2026-08-09
updated: 2026-08-09
type: bug
status: open
priority: normal
---

# publish config path: --help says ~/.config but macOS resolves to ~/Library/Application Support

_Source: src/cli.rs load_publish_config_

## Description

`glasspad publish` resolves its config file via `dirs::config_dir()` (src/cli.rs ~1906), which on macOS is `~/Library/Application Support/glasspad/config.yaml` — but the `publish --help` text and the doc comment at src/cli.rs:1676,1711,1722 (and skill.md) all state `~/.config/glasspad/config.yaml` unconditionally. On a macOS host a user who follows the help puts config.yaml under ~/.config, the binary looks under ~/Library/Application Support, finds nothing, and publish fails with `missing_server` even though the file exists.

Observed 2026-08-09 while deploying glasspad.maalla.dev: config at ~/.config/glasspad/config.yaml was ignored on macOS (gertrud); moving it to ~/Library/Application Support/glasspad/config.yaml fixed it.

Fix options (pick one, cross-platform-consistent):
1. Honor $XDG_CONFIG_HOME and ~/.config on all platforms (many CLIs do this; matches the help text and keeps one documented path), OR
2. Keep dirs::config_dir() but make the --help text + doc comments + skill.md platform-accurate (show the macOS path).

Option 1 gives users one path to remember and matches every doc string already shipped; option 2 is the smaller change. Either is fine — the bug is that docs and behavior disagree on macOS.
