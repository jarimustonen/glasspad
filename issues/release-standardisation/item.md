---
created: 2026-08-15
updated: 2026-08-15
type: chore
status: done
priority: normal
closed: 2026-08-15
closed_by: claude
commits:
- hash: 18564ae
  summary: 'docs(release): standardise onto ossctl release pattern'
---

# Standardise release infra onto ossctl pattern

## Description

Glasspad slice of homebase issue cross-repo-release-standardisation. Bring glasspad's release setup onto the ossctl-style pattern: cargo-dist dist-workspace.toml, tag-triggered release.yml (cargo-dist) + publish-crates.yml, and the operating-policy grants (autonomous release, pull-rebase-push) plus the cross-platform (macOS AND Linux) requirement.

On inspection the CI/config half was already in place from the 0.2.x cycle (dist-workspace.toml, release.yml, publish-crates.yml, CARGO_REGISTRY_TOKEN secret). release.yml regenerates clean from dist-workspace.toml via 'dist generate' (in sync). This slice fills the remaining doc gap and corrects a stale rationale comment:

- AGENTS.md: add the 'Cross-platform is a hard requirement (macOS AND Linux)' operating-policy paragraph matching ossctl canon, documenting glasspad's deliberately narrower binary matrix (gnu not musl; mac on a self-hosted runner; no Windows/Intel-mac binary; source path 'cargo install glasspad' covers the rest) and marking the custom-runner override as personal/non-standard infrastructure.
- dist-workspace.toml: correct the gnu-vs-musl rationale comment (was wrongly attributing it to reqwest->native-tls/OpenSSL; reqwest actually uses rustls-tls).

Reviewed via /llm-review (gemini + gpt-5.6): three confirmed factual-accuracy findings applied. Green gate: cargo fmt --all --check OK; dist plan valid; release.yml in sync; workflows valid YAML.

## Resolution

### 2026-08-15T04:25:30Z · @claude

Doc slice complete: cross-platform operating-policy paragraph added, gnu-vs-musl rationale corrected, release infra verified equivalent to ossctl pattern and in sync. Fulfils the glasspad slice of homebase cross-repo-release-standardisation.
