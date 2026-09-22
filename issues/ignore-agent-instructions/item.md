---
created: 2026-09-22
updated: 2026-09-22
type: bug
reporter: jari
status: fixed
priority: normal
lane: template-rendering
lane_seq: -100
closed: 2026-09-22
closed_by: agent
---

# Directory publish rejects agent instruction files

_Source: glasspad publish <directory>_

## Description

Publishing a documentation directory fails when it follows the common repository convention of keeping `AGENTS.md` and a `CLAUDE.md` symlink beside reader-facing Markdown pages. Glasspad currently treats every Markdown entry as a publishable artifact before filtering repository-management files, so `AGENTS.md` fails slug validation and `CLAUDE.md` would subsequently fail symlink validation.

Directory publication should ignore recognized agent-instruction files (`AGENTS.md` and `CLAUDE.md`) rather than forcing producers to move documentation, maintain a duplicate publication tree, or weaken their repository conventions. The ignored files must not appear as pages or assets, and ordinary invalid slugs or unrelated symlinks must remain hard errors.

## Reproduction

From `/home/jari/Sources/native-agent-host` at revision `7026bbef131b8a9bb55f9bbae697b3fe7441c5af`:

```sh
cd docs
glasspad publish . --json
```

Observed with Glasspad 0.17.4:

```json
{"error":{"code":"invalid_space","message":"cannot publish .: invalid slug \"AGENTS\" (./AGENTS.md): a slug (filename stem) must be lowercase [a-z0-9-], start alphanumeric, and be ≤64 chars"},"schema_version":1}
```

The same directory contains `CLAUDE.md -> AGENTS.md`, which must also be ignored safely instead of traversed or rejected as publication content.

## Quick Test

- Add a fixture space containing `index.md`, `AGENTS.md`, and `CLAUDE.md -> AGENTS.md`.
- Verify directory build/publish succeeds and emits only the reader-facing `index` artifact.
- Verify neither instruction filename is present in the manifest, navigation, assets, or hosted payload.
- Verify an unrelated uppercase Markdown filename and an unrelated symlink still fail under the existing strict validation and security rules.
- Verify direct publication of a specifically named instruction file remains explicit and well-defined (either supported as a single-file input or rejected with an intentional error); the directory-ignore behavior must not accidentally broaden symlink traversal.
