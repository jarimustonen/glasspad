---
created: 2026-08-06
updated: 2026-08-06
type: feature
status: in-progress
priority: low
related: ['@version-command']
---

# Wire build-time git commit into glasspad version output

## Description

The 0.3.0 round added \`glasspad version\` / \`-V/--version\` with a nested \`--json\` envelope \`{schema_version, data:{name,version,commit}, warnings}\` (issue version-command, landed 2026-08-06). The \`commit\` slot is always \`null\` today — the build does not capture the git SHA at compile time.

## Ask
Populate \`data.commit\` with the short git SHA of the build (e.g. via a \`build.rs\` reading \`git rev-parse --short HEAD\`, or the \`vergen\`/\`GIT_SHA\` env pattern the sibling CLIs use). Fall back to \`null\` cleanly when built outside a git checkout (e.g. from a crates.io tarball).

## Why
Lets tooling pin the exact build behind a released version for debugging. Matches the sibling CLIs (issuectl/ossctl/orchestratectl) which expose a commit stamp. Low priority — the version/number answer already works.
