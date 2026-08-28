# TODO — Glasspad handoff

Round-by-round handoff for `/stint`. **The issue tracker is the source of truth** —
`issuectl dag` is authoritative for scheduling; this file is orientation only.
Release history lives in `CHANGELOG.md` and git; closed issues keep their own detail.

## Where we are

**0.17.2 is released and live**, verified on crates.io, GitHub Release (12 assets), and the
Homebrew formula. The tag-triggered publish and release workflows and `main` CI all completed
successfully. The tree is clean, `main` is pushed, and no Glasspad worker owns preserved work.

The 2026-08-21..26 work completed the internal follow-ups from the previous handoff and the
public-repository release pass:

- 0.17.0 shipped the per-command CLI module split, pure artifact-host extraction into
  `glasspad-core`, honest hosted-submission acknowledgement state, and the explicit CI-owned
  publication contract. The package remains deliberately one published crate with two roots.
- 0.17.1 shipped the contributor-facing code map and issue forms, polished README and producer
  docs, and migrated current release-tooling references to Shipshape. The repository's public
  front door and community profile are now complete.
- 0.17.2 fixed the installed `doctor` bundle-version mismatch. A test now requires bundled
  `skill.md` metadata to equal `CARGO_PKG_VERSION`; future release bumps must update both before
  running the full gate.

## ▶ Start here

Run `issuectl dag --json --reservations '[]'` for the authoritative schedule; this file records
orientation only. All open non-epic work has now been human-dispositioned and is represented in
the DAG. The next product direction is template and presentation quality:

- Clarify that hosted full documents are verbatim **inside the artifact iframe**, while the
  trusted Glasspad space shell remains around them. This is a small documentation correction,
  not a request for a new chrome-free hosting mode.
- Fix the base template header: it currently ignores day/night theme changes and repeats the
  article/text title.
- Continue **`base-template-gallery`** from its draft design brief. Jari still needs to confirm
  or revise the proposed six topic areas (`prose`, `dashboard`, `report`, `board`, `index`,
  `table`); then complete the example/background package for the external design AI and proceed
  with integration.

One non-repository manual task remains: set the GitHub social-preview image in the web UI
(`brand/logo.png` or the README screenshot are suitable sources).

## Standing lessons

- **No speculative hardening.** A review finding whose own justification is "another layer
  already validates this", "does not happen on default settings", or that requires an attacker
  with write access to the server's own storage **is not work** — close it `wontfix`, do not
  lane it. Four such issues were closed on 2026-08-16 without any code being written. Put this
  filter in every brief that includes `/llm-review`, or the next review round regenerates them.
  Filed upstream as homebase `triage-plausibility-filter`.
- **Rank by frequency, not severity.** "Happens on every publish" beat "could corrupt data
  under a rare crash" in every call of that round.
- **Every brief gets a `CHANGELOG.md` line in its done criteria.** Three units landed
  2026-08-16 without one, leaving `[Unreleased]` empty.
- **Tell each brief WHICH changelog section to write to when a release may be cut mid-round.**
  On 2026-08-20 the `help-json` unit wrote its entry into the **already-published 0.15.0**
  section, because 0.15.0 was cut while its branch was alive and `[Unreleased]` had been
  emptied under it. The changelog then falsely claimed a feature shipped a release early; the
  orchestrator caught it only by reading the section before cutting 0.16.0. Two units in one
  round put a line in the wrong place, so this is a pattern, not a slip: **verify `[Unreleased]`
  contents against what actually landed before every release cut.**
- **Re-verify a worker's green claim against the FULL `./test-security.sh`** (Phase 1 + Wave
  2a), never Phase 1 alone. A release was once halted because a worker's "green" covered only
  part of the suite.
- **"User-specific" means the maintainer's SETUP, not the maintainer's ACCOUNT (Jari,
  2026-08-18).** The repo living under a personal GitHub account is fine; documentation that
  describes the maintainer's personal machines and configuration is not. *"On aivan eri asia
  että se on näin kun että projektissa puhuttaisiin mun omasta henkilökohtaisesta setupista ja
  konffeista."* Concretely: "install this on the haapa server" is a defect;
  `github.com/jarimustonen/glasspad` in an install command is not. The real defect class is
  personal machine names, personal filesystem paths, private sibling-repo names, internal URLs,
  and personal contact addresses. History is fine — old records need no scrubbing for their own
  sake. (`audit-no-user-specifics` over-reached by forbidding account handles outright; its
  closed analysis records which specific items were settled and why.)
- **Verify a landed unit's claims yourself, especially against a false premise in your own
  brief.** On 2026-08-20 the orchestrator's brief asserted that an existing security check
  forbade a wildcard bind on `host-serve`. It does not — that check guards `loopback serve`.
  The worker complied and added a **new** refusal that had never existed, silently changing
  production behaviour nobody asked for. It was caught by running the binary rather than
  trusting the report, and reverted to a loud warning (2026-08-20). Two lessons: state
  constraints in a brief only for behaviour you have actually confirmed, and smoke-test a
  landed change directly instead of reading its summary.
- **Release before a large restructure, not after.** 0.16.0 was cut ahead of `s22` precisely
  because a crate split touches `Cargo.toml`, `dist-workspace.toml`, and the published package
  — a broken pipeline would only surface at tag time, with finished user-visible work stuck
  behind the debugging. Publishing first kept a proven-good pipeline and isolated the risk.
- **Decided 2026-08-17 — do NOT file the hosted mutation-lock narrowing.** Staging/fsync
  outside the lock, holding it only for the pointer flip, would only pay off under *concurrent*
  publishes, and there is one publisher. It also breaks GC's "any dotted staging dir is a crash
  remnant" invariant and the snapshot read-modify-write. Revisit only on evidence: more than
  one API key, or measured publish latency showing `gc` contention.

## Gotchas

- **Toolchain.** Dependencies (`idna_adapter` → `icu_*`) need rustc ≥1.86. The machine default
  was corrected to `stable` on 2026-08-17; if a plain `cargo` ever fails on MSRV, run the gate
  under `rustup run stable`.
- **`version_cli` false-fail.** `cargo test` can report a spurious `commit … got: Null` on an
  incremental local build — `rm -rf target/debug/build/glasspad-*` and re-run. Clean CI builds
  never hit it. (Also in root `AGENTS.md`.)
- **Never re-add `GIT_CONFIG_GLOBAL` to a runner `.env`** — that is what broke macOS release
  builds. The durable fix and full write-up live in operator configuration outside this repo.

## Cutting a release

`git push origin vX.Y.Z` triggers **both** workflows off the tag — `release.yml` (cargo-dist:
binaries + Homebrew formula) and `publish-crates.yml` (crates.io). Bump `Cargo.toml`, finalize
the `CHANGELOG.md` entry, commit, then tag and push. No `gh release create` step: cargo-dist
creates the Release itself.

- The macOS build runs on the configured self-hosted runner.
- Cut a tag **once** and let it finish; don't re-tag mid-run.
- `release.yml` has **no `workflow_dispatch`** — a failed release can only be re-run
  (`gh run rerun <id> --failed`) or re-triggered by re-pointing the tag.
- Secrets already set: `CARGO_REGISTRY_TOKEN`, `HOMEBREW_TAP_TOKEN`.
- The agent has **standing release autonomy** (`AGENTS.md` → Operating Policy): may decide to
  release and cut it, gated on green checks, without asking.

## Notes for the orchestrator

- Keep `main` clean — commit issue/status changes immediately.
- The repo is **public**; treat commits and history as public.
- Verification is local: `cargo build`, serve a space, reload, and `./test-security.sh` as the
  regression gate after any host/header/CSP/bridge change. `./test-browser.sh` for ad-hoc
  browser automation (check `./test-browser.sh errors` first).
- `/shipshape-*` skills (over `shipshape`) drive release/readiness work; `shipshape audit` scores gaps.
- Track all planning under the issue, never as loose files.

## Piialiisan bugiraportit

- [ ] 🐛 Piialiisan bugiraportti: Hosted full-document HTML shows Glasspad chrome — jari via Telegram ([`intake-bug-glasspad-a14803a38786`](issues/intake-bug-glasspad-a14803a38786/item.md))
- [ ] 🐛 Piialiisan bugiraportti: Prevent accidental duplicate spaces when republishing a source path — jari via Telegram ([`intake-feature-glasspad-36fb5d8417e9`](issues/intake-feature-glasspad-36fb5d8417e9/item.md))
