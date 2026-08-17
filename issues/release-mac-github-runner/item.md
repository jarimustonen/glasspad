---
created: 2026-08-07
updated: 2026-08-07
type: task
status: done
priority: high
commits:
- hash: 9deee1a
  summary: route mac build to macos-14 runner
closed: 2026-08-07
---

# Route release macOS build to a GitHub-hosted runner

## Description

The 0.3.0 release (2026-08-06) published to crates.io cleanly, but the cargo-dist
`release.yml` **failed** on its `aarch64-apple-darwin` job: `actions/checkout`
returned git HTTP 400 on the self-hosted runner because its global Git configuration
had a stale authentication entry. Two consecutive
runs failed identically. Because the mac build is upstream of global-artifacts /
host / homebrew / announce, **no GitHub Release `v0.3.0` and no Homebrew formula
update were produced** — only the two Linux binaries built.

`release.yml` has **no `workflow_dispatch`** (tag-push + PR only), so the release
cannot be re-triggered without either fixing the runner or moving the tag.

## Ask

Re-route the macOS build off the self-hosted runner onto a **GitHub-hosted
Apple-Silicon runner** so releases stop depending on personal infrastructure:

- In `dist-workspace.toml`, change `[dist.github-custom-runners] aarch64-apple-darwin`
  from `"self-hosted"` to `"macos-14"` (GitHub-hosted arm64). Update the stale
  custom-runner comment accordingly.
- Regenerate `.github/workflows/release.yml` with the pinned `dist` 0.28.2
  (`dist generate`) — never hand-edit the workflow. Verify the mac job's `runs-on`
  is `macos-14` and that regeneration introduced no other unintended changes.
- Keep target set unchanged (do NOT add `x86_64-apple-darwin` in this change).

## Notes / out of scope (user's call on return)

Completing the **0.3.0** GitHub Release + Homebrew is a separate maintainer decision:
either (a) fix the self-hosted runner's Git configuration and rerun the failed job (keeps
the tag), or (b) re-point the `v0.3.0` tag onto the commit carrying this runner fix
so it rebuilds on the GitHub-hosted runner. crates.io `glasspad 0.3.0` is already
published and permanent regardless.
