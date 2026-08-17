# User-specific facts audit

Audit completed 2026-08-17 against every tracked text artifact in the repository: source,
configuration, generated workflows, installed skill/prompt content, documentation, issue history,
examples, tests, and fixtures. The pass combined targeted searches for known identifiers with
broader searches for account handles, email addresses, named home directories, source-root
conventions, machine names, internal domains, repository URLs, and environment-dependent defaults.

## Findings fixed

- Replaced private producer-repository names throughout historical issues, examples, TODO, the
  changelog, and subsystem guidance with clearly fictional producer names.
- Replaced the internal hosted-service domain in issue records and a render regression fixture
  with `share.example.com`.
- Removed personal machine names, named home-directory paths, and references to private operator
  documentation. Historical runner incidents now describe the relevant role and failure without
  identifying a machine or filesystem layout.
- Replaced personal reporter/author identifiers in issue records with role identifiers such as
  `maintainer`.
- Replaced the personal security and Code of Conduct email address with repository-mediated private
  reporting instructions. The security fallback tells reporters how to request a private channel
  without publishing vulnerability details.
- Generalized comments that named private deployment automation. No production default in the CLI
  encoded a personal server, credential, bind address, path, or hostname: hosted publishing already
  requires `server` and `api_key` through flags, environment, or config and emits actionable errors
  when they are absent. The zero-config default remains neutral loopback operation.

After these fixes, targeted searches are clean except for the deliberately escalated items below.

## Deliberately retained pending discussion

### macOS release runner

`dist-workspace.toml` still maps `aarch64-apple-darwin` to `self-hosted`. The personal hostname and
operator-only repair path have been removed, but the built-in release default still assumes the
maintainer's infrastructure. Cargo-dist records runner routing in repository configuration and
regenerates `.github/workflows/release.yml`; it has no glasspad user-config layer. Removing the
mapping without a replacement would break the required macOS release build.

Recommendation: restore the previously used neutral GitHub-hosted `macos-14` routing in a dedicated
release-infrastructure change, verify an arm64 artifact in a dry-run/tag-equivalent workflow, and
only then remove the self-hosted mapping. Alternatives are an Actions repository-variable
expression plus an explicit preflight failure naming the required variable, or retaining the
current route as a documented infrastructure exception. The hosted runner is preferred because it
makes forks and releases portable without requiring out-of-band setup.

### GitHub/Homebrew namespace

The maintainer's GitHub account handle remains in package metadata, install instructions, the
cargo-dist Homebrew tap configuration and generated workflow, release records, and the bundled
public `issuectl` installation guidance. These are live public distribution coordinates rather
than guessed runtime defaults: replacing them with fictional values would break installs, package
provenance, release links, or the release pipeline. They nevertheless meet the issue's literal
definition of a personal account handle and cannot be moved into glasspad's runtime user config.

Recommendation: migrate glasspad, its Homebrew tap, and relevant public sibling repositories to a
neutral GitHub organization, then update metadata and regenerate cargo-dist output atomically.
Until such a namespace exists, retain the working coordinates as an explicit exception. A build-time
or Actions-variable indirection would hide the value from selected generated files but would leave
public install coordinates unresolved and make source releases non-reproducible, so it is not a
complete fix.

### Copyright attribution

`LICENSE` retains the maintainer's legal name in the MIT copyright notice. Replacing it with a
fictional value would make the legal attribution inaccurate, and runtime user configuration is not
an appropriate mechanism for license provenance.

Recommendation: retain accurate legal attribution as a narrow legal exception unless copyright is
formally assigned to a neutral project organization or legal entity, at which point the notice can
be updated deliberately.

## Verification scope

No production behavior, host policy, headers, CSP, or bridge logic changed. The only Rust change is
a test URL and neutralized comments. The full project green gate and full security suite are still
run because the regression fixture lives in the artifact-host module.
