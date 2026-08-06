---
created: 2026-08-05
updated: 2026-08-06
type: feature
status: in-progress
priority: normal
---

# Expose an installed-version command (glasspad --version / version)

## Description

## Problem

glasspad exposes no way to query its own installed version: `glasspad --version`, `glasspad -V`, and `glasspad version` all error (`unexpected argument` / `unrecognized subcommand`). Subcommands are serve/create/open/data/skill/help only.

## Impact

The homebase fleet-updater provisions the shared CLIs on Linux (haapa + any Linux clone) via each tool's cargo-dist release installer, version-gated on the latest release tag so a re-run is a cheap no-op. issuectl (`issuectl --version`), ossctl (`ossctl version`), and orchestratectl (`orchestratectl version` → JSON) all expose a version, so their Linux hooks compare installed-vs-latest directly. glasspad does not, so `dotfiles/setup.d/glasspad.sh` had to fall back to writing a marker file (`~/.local/state/glasspad.installed-tag`) after each install to gate the next run — a workaround the siblings don't need.

## Ask

Add a standard version command — `glasspad --version` (clap's built-in `version` on the top-level command) and/or a `glasspad version` subcommand, ideally with a `--json` envelope like orchestratectl/ossctl. Then the fleet hook can drop the marker workaround and gate like the others.
