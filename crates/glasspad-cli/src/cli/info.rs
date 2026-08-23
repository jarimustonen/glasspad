use super::runtime::*;
use super::skill::bundled_skill_metadata;
use super::*;

// --- version --------------------------------------------------------------

/// `glasspad version` (and `glasspad --version` / `-V`) — report the installed
/// CLI version so tooling (the deployment fleet updater) can version-gate installs,
/// matching the sibling CLIs (`issuectl --version`, `shipshape version`,
/// `orchestratectl version`).
///
/// Under `--json`, emit the AI-first §10 envelope with the version payload
/// **nested under `data`** — `{schema_version, data: {name, version, commit,
/// supported_schemas, skills}, warnings}` — the same shape orchestratectl/shipshape
/// `version` use, so the cross-tool fleet-updater reads `.data.version`
/// uniformly across every tool rather than special-casing glasspad. Otherwise a
/// plain `glasspad <version>`
/// line on stdout (the data channel). Both the subcommand and the `--version` /
/// `-V` flag route here (main dispatches the flag manually), so all three honor
/// `--json` identically.
///
/// `version`/`name` are the compile-time `CARGO_PKG_VERSION`/`CARGO_PKG_NAME`,
/// the single source of truth shared with `Cargo.toml`. `commit` is the build
/// provenance: `build.rs` resolves the 12-char short SHA of the repository HEAD
/// and emits it under the internal carrier `GLASSPAD_COMMIT` at compile time
/// when built inside this crate's git checkout, and `option_env!` reads it here.
/// (The public `GLASSPAD_BUILD_COMMIT` override input is consumed and validated
/// by `build.rs`, never read here — a bare `option_env!` reads the ambient
/// compile environment, so reading that name directly would bypass validation.)
/// Outside a checkout (a crates.io tarball / `cargo install` with no `.git`, or
/// git missing) the build script emits nothing, so `commit` reports `null`,
/// never a bogus or partial hash. It is the HEAD commit, not a guarantee the
/// binary matches that tree (a dirty working tree still reports its HEAD).
pub fn version(json: bool) {
    let name = env!("CARGO_PKG_NAME");
    let ver = env!("CARGO_PKG_VERSION");
    let commit = option_env!("GLASSPAD_COMMIT");
    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "data": {
                "name": name,
                "version": ver,
                // `Option<&str>` → a JSON string or `null`; the key is always
                // present so a strict consumer never hits a missing-field error.
                "commit": commit,
                "supported_schemas": SUPPORTED_ENVELOPE_SCHEMAS,
                "supported_schemas_by_name": {
                    "envelope": SUPPORTED_ENVELOPE_SCHEMAS,
                    "help": SUPPORTED_HELP_SCHEMAS,
                },
                // `skill list` uses this same accessor, so version drift audits and
                // skill discovery cannot report divergent bundled-skill metadata.
                "skills": bundled_skill_metadata(),
            },
            // Present (empty) for cross-command uniformity: callers read
            // `warnings` unconditionally across every envelope.
            "warnings": [],
        });
        emit_json_line(&payload);
    } else {
        println!("{name} {ver}");
    }
}
