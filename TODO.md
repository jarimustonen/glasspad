# TODO — Glasspad handoff

Round-by-round handoff for `/stint`. **The issue tracker is the source of truth** —
`issuectl dag` is authoritative for scheduling; this file is orientation only.
Release history lives in `CHANGELOG.md` and git; closed issues keep their own detail.

## Where we are

**0.18.2 is released** (2026-09-24), verified on crates.io, GitHub Release (12 assets),
and the Homebrew formula. Both tag-triggered CI workflows passed. `main` is pushed and clean.
The locally installed `glasspad` CLI currently reports 0.18.1; release verification is not
proof that the public hosted server has been deployed. Check live version before claiming the
new viewer chrome appears at an existing hosted URL.

The 2026-09-24 round delivered the six-template gallery in 0.18.0 and patches in 0.18.1/0.18.2:

- The external designer's original and incremental returns were reviewed against the six
  briefs; findings live in `issues/base-template-gallery/{return-review,update-review}.md`.
  The ZIP inputs are gitignored at `history/designer-return/` on this host.
- `prose`, `dashboard`, `report`, `board`, `index`, and `table` were integrated one at a time
  into the single published package. Each slice passed Rust, publish dry-run, the complete
  51-check + Wave 2a security gate and real-host browser checks. The final 0.18.0 tree passed
  the full release gate again before the tag was pushed. `base-template-gallery` is `done`.
- Jari accepted the real `relative-markdown-assets` bug for immediate repair. Markdown-authored
  relative `assets/` image/file links now resolve through the scanned same-space asset route
  in hosted and loopback mode; local hosted browser checks decoded an AVIF, and the full gate
  passed again after the 0.18.1 version bump. The public hosted deployment was not touched.
- The native-agent-host visual-acceptance Markdown and three AVIFs were tested on the actual
  hosted service with 0.18.1: images decoded and returned HTTP 200 from same-space assets.
  The public test URL is `https://glasspad.maalla.dev/p/fjzftqivlexpzxvv53mswzlvja/visual-acceptance`.
- Jari rejected the old viewer header and requested theme controls in the sidebar. 0.18.2
  removes the redundant top strip; desktop uses a trusted left sidebar (prose TOC remains
  sandboxed on the right), mobile uses a bottom dock. Full gate and local 360/1280 browser
  inspection passed; `viewer-chrome-sidebar` is `done`.
- Designer `space-nav/` was intentionally not integrated. Wide print tables remain best-effort;
  raw HTML URLs and relative `.md` page navigation were not changed. Visual acceptance of
  the revised chrome on the public host is not yet recorded.

The 2026-09-22 directory-instruction filtering shipped in 0.17.5. The old superseded worker
branches remain historical cleanup context only; no session-owned run is unsettled.

## ▶ Start here

Run `issuectl dag --json --reservations '[]'` for the authoritative schedule; this file records
orientation only. `relative-markdown-assets` is currently unscheduled context awaiting a human
lane-or-close decision; it is not accepted, scheduled, or executable work.

The six built-ins, Markdown asset fix and revised viewer chrome are in 0.18.2. **The DAG has
no open scheduled or unscheduled issues.** Next, converge the installed CLI and public hosted
service through the owning Homebase deployment interface after confirming its policy/target;
the old public test URL still serves the running server's shell until that service is updated.
Then inspect light/dark/phone examples on that URL and ask Jari for visual feedback. Do not
mistake a published crate for a completed production deployment. Designer `space-nav/` was
only a proposal, not accepted work.

One unrelated manual task remains: set the GitHub social-preview image in the web UI
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
