# TODO — Glasspad handoff

Round-by-round handoff for `/stint`. **The issue tracker is the source of truth** —
`issuectl dag` is authoritative for scheduling; this file is orientation only.
Release history lives in `CHANGELOG.md` and git; closed issues keep their own detail.

## Where we are

**0.14.0 is released and live** (crates.io + GitHub Release + Homebrew).

**Three units have landed since and are UNRELEASED.** `Cargo.toml` still says `0.14.0`;
`CHANGELOG.md [Unreleased]` is written and describes the 0.15.0 content. Jari's steer
(2026-08-17): **run the next stint first, then release.** Cutting 0.15.0 needs only a version
bump, changelog finalize, and a tag push — the full green gate was already verified on the
current `main`.

Landed, unreleased:
- `cli-canon-config` — `glasspad config path` / `config show` (`--json`), each effective value
  with its provenance (flag / env / config file / default). **`api_key` is reported only as
  `<set>`/`<unset>`**, enforced by `tests/config_cli.rs:60`.
- `hosted-snapshot-arc-sharing` — `Snapshot.spaces` is `BTreeMap<String, Arc<Space>>`, so
  publish/update/round-push no longer deep-copy every body under the mutation lock;
  `MAX_PAGES` now enforced on scan/load too.
- `space-custom-template` — a space can declare a producer-supplied template applied to every
  markdown page; sidebar, landing index, and TOC rail still work; a space declaring no
  template renders as before. (Landed without the `base-templates-design` process an earlier
  handoff had recommended.)

## ▶ Start here

Clean tree, `main` pushed, nothing in flight. **Four lanes, four ready heads** — run
`issuectl dag` for the live picture.

**1. `repo-hygiene` → `audit-no-user-specifics`** (task, **high** — the only high-priority
item open). The repo is public; audit it for user-specific facts that must not ship, and move
any into user config. The rule: *overridability does not launder a user-specific default* — an
unset default is still whatever ships; the correct built-in default is neutral/absent with an
actionable error naming the config key. **Lead already identified:**
`dist-workspace.toml`'s `[dist.github-custom-runners]` routes the macOS release build to the
personal self-hosted `hauis` runner, and `AGENTS.md` itself calls that a "personal /
non-standard infra override" — exactly the pattern the issue forbids.

**2. `cli-canon` → `cli-canon-version-payload`**, then `skill-subcommand`, `doctor`,
`help-json`, `s22`. **Jari's decision 2026-08-16: all six get done**, over an orchestrator
recommendation to close three as ceremony. Recorded so it is not re-litigated —
`version-payload` would report constants (one schema version, one shipped skill);
`skill list` would list that one skill (the `skill` verb already exists, so this is smaller
than the issue implies); `s22` is a crate split of a ~4.5k-line `src/cli.rs` that the canon
itself marks "should", never a release gate. **Decision made — build them.**

**3. `hosted-hardening` → `hosted-idem-sweep-robustness`** — now the lane's only issue, and
already **narrowed** (2026-08-17) to the one real defect: `sweep_mappings` deletes a mapping on
*any* `read_capped` error including transient EMFILE/EACCES, discarding duplicate-publish
protection during exactly the retry an idempotency key exists for. Fix: delete only on
`NotFound` or an explicit parse/validation failure. The symlink, empty-tenant-reap, and
invalid-record items are recorded as out of scope **in the issue** — do not let a later review
reintroduce them. `hosted-gc-swap-on-partial-fsync` was closed `wontfix` the same day (failing-
disk trigger, restart-clearable), which is why this lane is one deep.

**4. `space-polish` → `docsite-autolink-convention`** (feature, low). Mostly docs: describe the
"preprocess markdown before publish" seam and confirm which author link classes (e.g.
`<a class="xref">`) survive into rendered prose for theming. glasspad does not own
glossary/xref logic; aggountant keeps a thin preprocessor.

**Open decision carried:** file the hosted-submit **async/webhook push**
(push-to-a-departed-agent) as a future design issue, or drop it? Unfiled, Jari's call.

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
- **Re-verify a worker's green claim against the FULL `./test-security.sh`** (Phase 1 + Wave
  2a), never Phase 1 alone. A release was once halted because a worker's "green" covered only
  part of the suite.
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
  builds. Durable fix and full write-up: `homebase/infra/machines/hauis.md`.

## Cutting a release

`git push origin vX.Y.Z` triggers **both** workflows off the tag — `release.yml` (cargo-dist:
binaries + Homebrew formula) and `publish-crates.yml` (crates.io). Bump `Cargo.toml`, finalize
the `CHANGELOG.md` entry, commit, then tag and push. No `gh release create` step: cargo-dist
creates the Release itself.

- The macOS build runs on the self-hosted `hauis` runner.
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
