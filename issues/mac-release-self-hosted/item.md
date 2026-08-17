---
created: 2026-08-09
updated: 2026-08-09
type: task
status: done
priority: normal
commits:
- hash: 468a26dd4678a3db12bf17e3ec92b2cf84f04eba
  summary: 'ci(dist): revert mac release build to self-hosted hauis runner'
closed: 2026-08-09
---

# Revert macOS release build back to self-hosted hauis runner

## Description

`release-mac-github-runner` (commit `9deee1a`) routed the macOS release build to a
GitHub-hosted `macos-14` runner in `dist-workspace.toml`, because the self-hosted
`hauis` runner was failing (git HTTP 400 / auth-placeholder errors). That runner is
now **durably fixed** (2026-08-09: `~/.gitconfig` `[include]` split replaced the broken
`GIT_CONFIG_GLOBAL` override; write-up in `homebase/infra/machines/hauis.md`) and hauis
is the **intended** mac build machine. So the `macos-14` routing should be reverted.

## Fix

In `dist-workspace.toml`, restore the mac target routing to the self-hosted runner
(the pre-`9deee1a` state):

```toml
[dist.github-custom-runners]
aarch64-apple-darwin = "self-hosted"
```

Keep the two Linux targets on GitHub-hosted ubuntu runners (unchanged). Then run
`dist generate` to regenerate `.github/workflows/release.yml` (the mac matrix row must
go back to `runner: self-hosted`, which matches hauis — the only self-hosted runner).
Update the explanatory comment block in `dist-workspace.toml` to reflect that hauis is
fixed and owns mac builds again. **Do not** touch `publish-crates.yml`. Green gate:
`cargo fmt --all --check` + the workflow YAML must be valid (a `dist generate` diff only).

## Comments

- No version bump; this is CI-config only. Verify `git diff` touches only
  `dist-workspace.toml` + `.github/workflows/release.yml`.
- The next actual release (a future tag push) will exercise it; no release is cut here.

