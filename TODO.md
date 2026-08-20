# TODO — Glasspad handoff

Round-by-round handoff for `/stint`. **The issue tracker is the source of truth** —
`issuectl dag` is authoritative for scheduling; this file is orientation only.
Release history lives in `CHANGELOG.md` and git; closed issues keep their own detail.

## Where we are

**0.16.0 is released and live** (crates.io + GitHub Release + Homebrew), verified on all three
channels. 0.15.0 shipped earlier the same round. Nothing is landed-but-unreleased except the
internal `s22` change described below.

The 2026-08-17..20 stint ran nine units to completion. What it delivered, in two themes:

**glasspad is now self-describing for agents** (all in 0.16.0). `skill list` / `skill print` /
`skill install` expose the bundled operating manual; `doctor` gives a read-only health check
with `--json` per-check records and exit 1 on failure; `--help --json` exposes the clap-derived
command tree, flags, examples and env mappings. These share one source of truth with
`version --json`'s `supported_schemas` + `skills[]` — a test pins the agreement, so do not
introduce a second list.

**The public-repo audit is done and its decisions are settled** — see Standing lessons for the
rule that resolved it. Also landed: the hosted idempotency sweep no longer discards a mapping
on a transient read error; `host-serve` binds ephemeral ports race-free and derives its public
origin; the producer preprocessing seam for markdown spaces is documented.

**Internal, unreleased:** `cli-canon-s22` split the crate into `crates/glasspad-core` (pure: no
clap, no `std::fs`, no `SystemTime::now`, injected `Clock`) and `crates/glasspad-cli`, kept as
**one published package** so `cargo publish` and `cargo install glasspad` are unchanged. That
packaging compromise is deliberate and correct — do not "fix" it into two published crates.
Note it also changed the library's CSV entry point to take bytes instead of a generic `Read`
(a breaking library-API change, accepted as harmless since glasspad is used as a CLI).

## ▶ Start here

Clean tree, `main` pushed at `519ca7a`, nothing in flight. Run `issuectl dag` for the live
picture — this file deliberately records no schedule.

**The `s22` split under-delivered, and the follow-up is already filed.** Measurement after it
landed: core is 1674 lines against 26917 on the CLI side, and `cli.rs` is still **5049 lines** —
the crate boundary moved without shrinking the file that actually hurts. Two issues capture the
maintainer's decision (2026-08-20) on what to do about it:

- **`cli-module-split`** (high) — split the 5049-line `cli.rs` into per-command-group modules,
  leaving a thin dispatch layer. This is **not** canon compliance; it was chosen on its merits.
  That file's size is what forced nearly every unit of this stint into a single sequential lane,
  so splitting it widens the parallelism available to every future round. The maintainer asked
  for it at the head of the work.
- **`artifact-host-core-extract`** — move the pure `artifact_host` logic (HTML wrapping,
  sanitization, shell rendering, templates: 7342 lines, only 33 I/O touchpoints) into core. It
  is nearly pure *and* security-critical, so it is the piece of §22 genuinely worth doing.

**Do not pursue "§22 to completion" as a goal.** `hosted` (8481 lines, **280** I/O touchpoints)
is a durable store plus an HTTP surface — it *is* the I/O edge §22 means, and it stays on the
CLI side permanently. A future canon audit flagging it should be rejected, not actioned. This is
recorded in `artifact-host-core-extract` too.

**The hosted return-channel question is settled and filed.** The product model is unchanged:
**the agent listens.** No push-to-a-departed-agent, no outbound webhook — both were considered
and rejected (*"Sopimus on, että agentti kuuntelee"*). What remains is `submission-ack-status`:
when nobody has collected a submission, say so **on the page**. Note the constraint recorded in
that issue — the status read is reachable from the published page, so it may reveal nothing but
the caller's own submission's delivery state.

**Needs scheduling triage:** `contract-declare-ci-publish-surface` (arrived from an ossctl stint,
concerns this repo's release-contract declaration) and `submission-ack-status` are both active
but unlaned.

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
  and personal contact addresses. **Decided and closed 2026-08-18: keep the self-hosted macOS
  runner, keep the personal GitHub/Homebrew namespace, keep the LICENSE legal name.** Do not
  re-escalate these; `audit-no-user-specifics` over-reached by forbidding account handles
  outright. Also: history is fine — old records need no scrubbing for their own sake.
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
- `/oss-*` skills (over `ossctl`) drive release/readiness work; `ossctl audit` scores gaps.
- Track all planning under the issue, never as loose files.
