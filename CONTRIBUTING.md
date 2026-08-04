# Contributing to glasspad

Thanks for your interest in glasspad — an AI-friendly, loopback-only HTML-artifact
host. Contributions of all kinds are welcome: bug reports, fixes, docs, and features.

## Reporting issues

- **Bugs and feature requests** are tracked with [`issuectl`](https://github.com/jarimustonen/glasspad)
  under [`issues/`](issues/) in this repository. Open an issue there (one directory per
  issue, `issues/<slug>/item.md`) or via a pull request that adds it.
- **Security vulnerabilities** — please do **not** open a public issue. Follow the
  coordinated-disclosure process in [`SECURITY.md`](SECURITY.md).

## Development setup

glasspad is a single Rust crate. You need a recent stable Rust toolchain
(`rustup`); the code uses the 2024 edition.

```bash
git clone https://github.com/jarimustonen/glasspad
cd glasspad
cargo build
```

## The green gate

Every pull request must pass these checks before it can merge — the same gates CI runs:

```bash
cargo fmt --all --check                       # formatting
cargo clippy --workspace --all-targets -- -D warnings   # lints
cargo test --workspace                        # unit + integration tests
./test-security.sh                            # the security-contract browser suite
```

`./test-security.sh` is the regression gate for glasspad's core promise — every HTML
artifact renders in a null-origin sandboxed iframe. Any change to the artifact host,
HTTP headers, CSP, or the injected bridge **must** keep it green. See
[`AGENTS-GUI-DEBUGGING.md`](AGENTS-GUI-DEBUGGING.md) for browser-automation setup.

## Pull request flow

1. Branch from `main`.
2. Make your change; keep commits focused and the tree green.
3. Open a pull request against `main`. Describe what changed and why, and link the
   issue it addresses.
4. A maintainer reviews and merges once the green gate passes.

## Commit messages

Write a clear, imperative summary line (e.g. `fix(host): reject symlinked artifact paths`).
When a change relates to a tracked issue, reference it in the commit body with an
`issuectl` trailer (`Refs-Issue: <slug>` or `Fixes-Issue: <slug>`) so the work stays
linked to its issue.

## Changelog

You do not need to edit `CHANGELOG.md` yourself — it is **curated by the maintainers**
at release time. Describe your change clearly in the PR and it will be captured in the
next release's notes.

## Licensing

glasspad is licensed under the **MIT** license (see [`LICENSE`](LICENSE)). By
contributing, you agree that your contributions are licensed under the same terms
(inbound = outbound).

## Code of conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). By participating,
you are expected to uphold it.
