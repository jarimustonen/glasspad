//! The `glasspad` CLI surface (Wave 3a / Phase 3): `serve`, `create`, `open`.
//!
//! Follows the project's AI-first CLI conventions (`AGENTS-AI-FIRST-CLI.md`):
//! strict input validation with informative, actionable errors; a stable,
//! versioned `--json` envelope on every command; errors as a structured envelope
//! on **stderr** with a meaningful exit code (1 = user error, 2 = system error);
//! and no interactive prompts. Paths are plain positional args — no hidden global
//! state, so the commands compose.
//!
//! The commands are two server entry points, a browser opener, and a standalone
//! data helper:
//! * `serve <dir>` drives Phase 2 live directory serving (scan + watch + SSE).
//! * `create <file>` builds a one-artifact space from a single file and serves it
//!   live (its own single-file watch).
//! * `open <space>` opens a served space's URL in the browser.
//! * `data <file>` parses a legacy CSV/JSON/mbox file to JSON rows (no server).
//!
//! Fragment-vs-full-document detection is **not** re-implemented here: the content
//! route classifies each artifact at serve time (`artifact_host::wrap`), so a
//! file authored either way — full `<!doctype html>` document or bare fragment —
//! is served correctly whether it arrives via `serve` or `create`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use crate::artifact_host::space::{self, ScanError};
use crate::artifact_host::{self, ArtifactHost, wrap};
use crate::server;

/// The `--json` schema version (AI-first §10). Bump on any breaking change to an
/// envelope: removed/renamed field, changed type/nullability, or changed meaning.
pub const SCHEMA_VERSION: u32 = 1;

// --- Error contract -------------------------------------------------------

/// Emit a structured error and exit. Under `--json` the AI-first §10 error
/// envelope goes to **stderr** (stdout stays the data channel); otherwise a
/// `error: <message>` line. `exit_code` is 1 for user error, 2 for a system/IO
/// failure the caller cannot fix by correcting its input.
pub fn exit_error(
    json: bool,
    exit_code: i32,
    code: &str,
    message: &str,
    invalid_value: Option<&str>,
    expected: Option<Vec<String>>,
) -> ! {
    if json {
        let mut err = serde_json::Map::new();
        err.insert("code".into(), json!(code));
        err.insert("message".into(), json!(message));
        if let Some(v) = invalid_value {
            err.insert("invalid_value".into(), json!(v));
        }
        if let Some(e) = expected {
            err.insert("expected".into(), json!(e));
        }
        let payload = json!({ "schema_version": SCHEMA_VERSION, "error": err });
        // Never emit a blank line: if the (statically-serializable) envelope somehow
        // fails to serialize, fall back to the text form so the message is not lost.
        match serde_json::to_string(&payload) {
            Ok(s) => eprintln!("{s}"),
            Err(_) => eprintln!("error: {message}"),
        }
    } else {
        eprintln!("error: {message}");
    }
    std::process::exit(exit_code);
}

/// Map a scanner rejection to a stable error `code` and exit code, then emit it.
/// The human message is the `ScanError` `Display` (already informative and, for
/// the symlink/reserved/collision cases, keyword-greppable by `test-security.sh`).
fn exit_scan_error(e: &ScanError, json: bool) -> ! {
    let (code, exit) = match e {
        ScanError::NotADir(_) => ("not_a_directory", 1),
        ScanError::BadSpaceName(_) => ("invalid_space_name", 1),
        ScanError::Io(_, _) => ("io_error", 2),
        ScanError::Symlink(_) => ("symlink_rejected", 1),
        ScanError::Escapes(_) => ("path_escapes_root", 1),
        ScanError::ReservedSlug(_, _) => ("reserved_slug", 1),
        ScanError::BadSlug(_, _) => ("invalid_slug", 1),
        ScanError::DuplicateSlug(_, _) => ("duplicate_slug", 1),
        ScanError::FileTooLarge(_, _) => ("file_too_large", 1),
        ScanError::SpaceTooLarge(_) => ("space_too_large", 1),
        ScanError::TooManyEntries(_) => ("too_many_entries", 1),
        ScanError::UnsupportedFileType(_) => ("unsupported_file_type", 1),
        ScanError::ManifestTooLarge(_, _) => ("manifest_too_large", 1),
        ScanError::NotUtf8(_) => ("not_utf8", 1),
        ScanError::BadAssetName(_) => ("invalid_asset_name", 1),
        ScanError::Manifest(_, _) => ("invalid_manifest", 1),
    };
    exit_error(json, exit, code, &e.to_string(), None, None);
}

/// Print a JSON envelope line to stdout and flush it. The flush matters for the
/// long-running `serve`/`create`: their startup envelope must reach a piped
/// consumer *before* the process blocks serving, not sit in a block buffer.
fn emit_json_line(payload: &serde_json::Value) {
    use std::io::Write;
    let s = serde_json::to_string(payload).unwrap_or_default();
    println!("{s}");
    let _ = std::io::stdout().flush();
}

// --- version --------------------------------------------------------------

/// `glasspad version` (and `glasspad --version` / `-V`) — report the installed
/// CLI version so tooling (the homebase fleet-updater) can version-gate installs,
/// matching the sibling CLIs (`issuectl --version`, `ossctl version`,
/// `orchestratectl version`).
///
/// Under `--json`, emit the AI-first §10 envelope with the version payload
/// **nested under `data`** — `{schema_version, data: {name, version, commit},
/// warnings}` — the same shape orchestratectl/ossctl `version` use, so the
/// cross-tool fleet-updater reads `.data.version` uniformly across every tool
/// rather than special-casing glasspad. Otherwise a plain `glasspad <version>`
/// line on stdout (the data channel). Both the subcommand and the `--version` /
/// `-V` flag route here (main dispatches the flag manually), so all three honor
/// `--json` identically.
///
/// `version`/`name` are the compile-time `CARGO_PKG_VERSION`/`CARGO_PKG_NAME`,
/// the single source of truth shared with `Cargo.toml`. `commit` is the build
/// provenance when a release build injected it (`GLASSPAD_BUILD_COMMIT`), else
/// `null` — there is no `build.rs` git shell-out, so a crates.io / `cargo
/// install` build without the env var reports `null`, never a bogus hash.
pub fn version(json: bool) {
    let name = env!("CARGO_PKG_NAME");
    let ver = env!("CARGO_PKG_VERSION");
    let commit = option_env!("GLASSPAD_BUILD_COMMIT");
    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "data": {
                "name": name,
                "version": ver,
                // `Option<&str>` → a JSON string or `null`; the key is always
                // present so a strict consumer never hits a missing-field error.
                "commit": commit,
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

// --- serve ----------------------------------------------------------------

/// `glasspad serve [dir]` — serve a live directory as a space, or (with no dir)
/// the built-in fixtures. Binds loopback, then blocks serving until killed.
pub async fn serve(dir: Option<PathBuf>, port: u16, json: bool) {
    let host = Arc::new(ArtifactHost::new(port));

    // (name, nav slugs, home) when a live directory is served; None = fixtures.
    let live: Option<(String, Vec<String>, Option<String>)> = match &dir {
        Some(d) => match server::scan_named(d) {
            Ok((name, snap)) => {
                let sp = snap.space(&name).expect("scanned space is present");
                let info = (name.clone(), sp.slugs(), sp.home.clone());
                host.swap(snap);
                Some(info)
            }
            Err(e) => exit_scan_error(&e, json),
        },
        None => None,
    };

    // Bind before announcing: a port collision is surfaced as an error, and the
    // startup envelope is only printed once the port is actually held.
    let listener = match server::bind_loopback(port).await {
        Ok(l) => l,
        Err(e) => exit_error(
            json,
            2,
            "bind_failed",
            &format!("cannot bind 127.0.0.1:{port}: {e}"),
            Some(&port.to_string()),
            None,
        ),
    };

    if let Some(d) = dir {
        server::spawn_watcher(host.clone(), d);
    }
    emit_serving(json, port, live.as_ref());

    let app = server::build_app_with_host(port, host);
    if let Err(e) = server::serve_on(listener, app).await {
        exit_error(
            json,
            2,
            "serve_failed",
            &format!("server stopped with an error: {e}"),
            None,
            None,
        );
    }
}

/// Print the `serve` startup envelope. `--json` → one line to stdout (the data
/// channel); text → a line to stderr (a long-running process's stdout stays free
/// for a caller that pipes it). The command is long-running, so this is a startup
/// announcement, not a terminal result — the server then runs until killed.
fn emit_serving(json: bool, port: u16, live: Option<&(String, Vec<String>, Option<String>)>) {
    match live {
        Some((name, slugs, home)) => {
            let url = format!("http://127.0.0.1:{port}/{name}/");
            if json {
                let payload = json!({
                    "schema_version": SCHEMA_VERSION,
                    "serving": true,
                    "port": port,
                    "space": name,
                    "url": url,
                    "artifacts": slugs,
                    "home": home,
                    "warnings": [],
                });
                emit_json_line(&payload);
            } else {
                eprintln!(
                    "glasspad serving space '{name}' at {url} ({} artifact{})",
                    slugs.len(),
                    if slugs.len() == 1 { "" } else { "s" }
                );
            }
        }
        None => {
            let url = format!("http://127.0.0.1:{port}/");
            let warn = "no directory given: serving built-in fixtures only; \
                        pass a directory to serve a space";
            if json {
                let payload = json!({
                    "schema_version": SCHEMA_VERSION,
                    "serving": true,
                    "port": port,
                    "space": serde_json::Value::Null,
                    "url": url,
                    "artifacts": [],
                    "home": serde_json::Value::Null,
                    "warnings": [warn],
                });
                emit_json_line(&payload);
            } else {
                eprintln!("glasspad serving built-in fixtures at {url} ({warn})");
            }
        }
    }
}

// --- create ---------------------------------------------------------------

/// `glasspad create <file> [--name <space>]` — build a one-artifact space from a
/// single file and serve it live (a single-file watch reloads on edit). The space
/// name defaults to the file stem (validated) and can be overridden with `--name`.
pub async fn create(file: PathBuf, name: Option<String>, port: u16, json: bool) {
    let (space_name, html) = load_single_file(&file, name.as_deref(), json);
    // Report which authoring level was detected — the same classifier the content
    // route uses to decide wrap-vs-verbatim (design.md §4 / plan §4).
    let kind = if wrap::is_fragment(&html) {
        "fragment"
    } else {
        "full-document"
    };

    let host = Arc::new(ArtifactHost::new(port));
    host.swap(server::one_artifact_snapshot(&space_name, html));

    let listener = match server::bind_loopback(port).await {
        Ok(l) => l,
        Err(e) => exit_error(
            json,
            2,
            "bind_failed",
            &format!("cannot bind 127.0.0.1:{port}: {e}"),
            Some(&port.to_string()),
            None,
        ),
    };

    server::spawn_file_watcher(host.clone(), file, space_name.clone());
    emit_created(json, port, &space_name, kind);

    let app = server::build_app_with_host(port, host);
    if let Err(e) = server::serve_on(listener, app).await {
        exit_error(
            json,
            2,
            "serve_failed",
            &format!("server stopped with an error: {e}"),
            None,
            None,
        );
    }
}

/// Validate + read the single file `create` serves, resolving the space name.
/// Strict (AI-first §1): a missing path, a directory, a non-regular / oversize /
/// non-UTF-8 file, or an un-derivable/invalid space name each exits with an
/// informative envelope rather than a silent fixup. Returns `(space_name, html)`.
fn load_single_file(file: &Path, name_override: Option<&str>, json: bool) -> (String, String) {
    // Validate the space name FIRST (AI-first §1 fail-fast): the name comes from
    // `--name` or the file stem — neither needs the file contents — so an
    // immediately-detectable argument error is reported before any file I/O.
    let space_name = resolve_space_name(file, name_override, json);

    // `metadata` follows a symlink: the user named this file explicitly, so a
    // symlink to their own file is served (unlike a directory scan, where a
    // symlink can smuggle a file in from outside the space and is rejected).
    let meta = match std::fs::metadata(file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => exit_error(
            json,
            1,
            "no_such_path",
            &format!("no such file: {}", file.display()),
            Some(&file.display().to_string()),
            None,
        ),
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    };
    if meta.is_dir() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is a directory; `create` takes a single file — use `serve` for a directory",
                file.display()
            ),
            Some(&file.display().to_string()),
            None,
        );
    }
    if !meta.is_file() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is not a regular file (FIFOs, sockets, and devices are not servable)",
                file.display()
            ),
            None,
            None,
        );
    }
    if meta.len() > space::MAX_FILE_BYTES {
        exit_error(
            json,
            1,
            "file_too_large",
            &format!(
                "{} is {} bytes, over the {}-byte per-file limit",
                file.display(),
                meta.len(),
                space::MAX_FILE_BYTES
            ),
            None,
            None,
        );
    }

    // Bounded read: cap the allocation at `MAX_FILE_BYTES + 1` so a file that grows
    // past the limit between the stat above and the read (a concurrent writer)
    // cannot make us allocate an unbounded buffer before the size recheck fires.
    let bytes = match read_capped(file, space::MAX_FILE_BYTES) {
        Ok(b) => b,
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    };
    if bytes.len() as u64 > space::MAX_FILE_BYTES {
        exit_error(
            json,
            1,
            "file_too_large",
            &format!(
                "{} exceeds the {}-byte per-file limit",
                file.display(),
                space::MAX_FILE_BYTES
            ),
            None,
            None,
        );
    }
    let html = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => exit_error(
            json,
            1,
            "not_utf8",
            &format!(
                "{} is not valid UTF-8 (artifacts must be UTF-8 HTML)",
                file.display()
            ),
            None,
            None,
        ),
    };

    (space_name, html)
}

/// Resolve + validate the space name for `create`: the `--name` override, else the
/// file stem. Same grammar the router and scanner enforce, so `create` can never
/// mint a name they would reject. Exits with an informative envelope on failure.
fn resolve_space_name(file: &Path, name_override: Option<&str>, json: bool) -> String {
    let (from_flag, raw_name) = match name_override {
        Some(n) => (true, n.to_string()),
        None => (
            false,
            file.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string(),
        ),
    };
    if artifact_host::valid_space(&raw_name) {
        return raw_name;
    }
    let message = if from_flag {
        format!(
            "invalid --name {raw_name:?}: a space name must be lowercase [a-z0-9-], \
             start alphanumeric, be ≤64 chars, and not be reserved ({})",
            artifact_host::RESERVED.join(", ")
        )
    } else {
        format!(
            "cannot derive a valid space name from {}: {raw_name:?} is not a valid name \
             (lowercase [a-z0-9-], start alphanumeric, ≤64 chars, not reserved: {}). \
             Pass --name <space> to set one explicitly.",
            file.display(),
            artifact_host::RESERVED.join(", ")
        )
    };
    // No `expected` list: the space grammar is not a finite enum, and the reserved
    // names are a *deny* list — surfacing them under `expected` (an allowlist, per
    // AI-first §10) would mislead a caller into retrying with a reserved name. The
    // message already spells out the grammar + reserved set.
    exit_error(
        json,
        1,
        "invalid_space_name",
        &message,
        Some(&raw_name),
        None,
    );
}

/// Read at most `max + 1` bytes of `file` into memory (a bounded allocation). The
/// caller treats a returned length `> max` as over-limit; the `+1` lets it detect
/// "exactly at the cap vs. over" without ever buffering an unbounded file.
fn read_capped(file: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let f = std::fs::File::open(file)?;
    let mut buf = Vec::new();
    f.take(max + 1).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Print the `create` startup envelope (mirrors [`emit_serving`], plus the single
/// slug and the detected authoring `kind`).
fn emit_created(json: bool, port: u16, space: &str, kind: &str) {
    let url = format!("http://127.0.0.1:{port}/{space}/");
    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "serving": true,
            "port": port,
            "space": space,
            "slug": server::SINGLE_SLUG,
            "home": server::SINGLE_SLUG,
            "url": url,
            "kind": kind,
            "warnings": [],
        });
        emit_json_line(&payload);
    } else {
        eprintln!("glasspad serving '{space}' ({kind}) at {url}");
    }
}

// --- open -----------------------------------------------------------------

/// `glasspad open <space> [--port] [--no-browser]` — resolve a served space's URL
/// and open it in the browser. Pure and composable: it builds the URL from the
/// space name + port and launches the OS opener; it holds no state and does not
/// probe whether a server is actually up (that is the caller's `serve`/`create`).
pub fn open(space: String, port: u16, json: bool, no_browser: bool) {
    if !artifact_host::valid_space(&space) {
        exit_error(
            json,
            1,
            "invalid_space_name",
            &format!(
                "invalid space {space:?}: a space name must be lowercase [a-z0-9-], \
                 start alphanumeric, be ≤64 chars, and not be reserved ({})",
                artifact_host::RESERVED.join(", ")
            ),
            Some(&space),
            None, // see load_single_file: reserved names are a deny list, not `expected`
        );
    }
    let url = format!("http://127.0.0.1:{port}/{space}/");
    let launched = if no_browser {
        false
    } else {
        launch_browser(&url)
    };

    // A requested-but-failed launch must not look like a deliberate `--no-browser`:
    // surface it as a non-fatal warning (§4/§10) so the caller can tell them apart.
    // Exit stays 0 — the URL is still valid and printed for the caller to use.
    let mut warnings: Vec<String> = Vec::new();
    if !no_browser && !launched {
        warnings.push(
            "browser launch failed (no opener available or spawn failed); \
             the URL is still valid — open it manually"
                .to_string(),
        );
    }

    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "space": space,
            "port": port,
            "url": url,
            "browser_launched": launched,
            "warnings": warnings,
        });
        emit_json_line(&payload);
    } else {
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        if launched {
            println!("Opening {url}");
        } else {
            // Pipe-friendly: the bare URL on stdout so `open --no-browser` composes.
            println!("{url}");
        }
    }
}

/// Launch the OS browser opener. Returns whether the opener was spawned (not
/// whether a browser actually appeared — the child is fire-and-forget).
fn launch_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = url;
        return false;
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        std::process::Command::new(cmd).arg(url).spawn().is_ok()
    }
}

// --- data (legacy-format helper) ------------------------------------------

/// `glasspad data <file> [--format] [--meta]` — parse a legacy CSV/JSON/mbox
/// file into JSON rows on stdout. A standalone convenience over the old data
/// parsers (`glasspad::data`): the section-DSL server that once ingested these
/// formats is gone (Wave 5 / Phase 6), but the parsers remain useful for turning
/// such a file into rows a hand-authored HTML artifact can embed. Never starts a
/// server.
///
/// Output contract (AI-first §10): stdout is the data channel. Under `--json`, a
/// versioned envelope `{schema_version, format, path, row_count, rows[, meta]}`;
/// otherwise the bare rows array (pretty JSON) on stdout with a one-line human
/// summary on stderr. Errors go to stderr via [`exit_error`] with a stable `code`.
pub fn data(file: PathBuf, format: Option<String>, meta: bool, json: bool) {
    use glasspad::data::{infer, limits, types::Dataset};

    // Resolve the format: an explicit `--format` wins, else infer from extension.
    let fmt = match format.as_deref() {
        Some(f) => f.to_string(),
        None => match detect_data_format(&file) {
            Some(f) => f.to_string(),
            None => exit_error(
                json,
                1,
                "unknown_format",
                &format!(
                    "cannot infer a data format from {}: pass --format csv|json|mbox",
                    file.display()
                ),
                Some(&file.display().to_string()),
                Some(vec!["csv".into(), "json".into(), "mbox".into()]),
            ),
        },
    };

    // Read the file, bounded to a 50 MB safety cap. The parsers do not bound by
    // byte count — csv/json/mbox each cap by rows and columns — so this read cap
    // is the only byte bound; the errors below carry the parser's own message.
    let bytes = read_data_file(&file, json);
    // Errors carry a stable `(code, message)` so a UTF-8 failure keeps its own
    // `not_utf8` code instead of collapsing into the generic `parse_failed`.
    let parsed: Result<Dataset, (&'static str, String)> = match fmt.as_str() {
        "csv" => {
            glasspad::data::csv::parse_csv(std::io::Cursor::new(&bytes), limits::MAX_CSV_BYTES)
                .map_err(|e| ("parse_failed", e.to_string()))
        }
        "mbox" => glasspad::data::mbox::parse_mbox_bytes(&bytes)
            .map_err(|e| ("parse_failed", e.to_string())),
        "json" => match std::str::from_utf8(&bytes) {
            Ok(s) => {
                glasspad::data::json::parse_json_str(s).map_err(|e| ("parse_failed", e.to_string()))
            }
            Err(_) => Err((
                "not_utf8",
                format!("{} is not valid UTF-8 (JSON must be UTF-8)", file.display()),
            )),
        },
        // `--format` is a fixed enum and `detect_data_format` only yields these
        // three, so any other value here is a programming error, not user input.
        other => unreachable!("format resolved to csv|json|mbox, got {other:?}"),
    };
    let rows = match parsed {
        Ok(r) => r,
        Err((code, msg)) => exit_error(json, 1, code, &msg, None, None),
    };

    let meta_val = if meta {
        Some(infer::infer_dataset_meta(&rows))
    } else {
        None
    };

    if json {
        // `warnings: []` matches the serve/create/open envelopes so a consumer can
        // read the field unconditionally across commands.
        let mut payload = json!({
            "schema_version": SCHEMA_VERSION,
            "format": fmt,
            "path": file.display().to_string(),
            "row_count": rows.len(),
            "rows": rows,
            "warnings": [],
        });
        if let Some(m) = &meta_val {
            // The envelope is a JSON object literal above, so this always succeeds.
            if let Some(obj) = payload.as_object_mut() {
                obj.insert(
                    "meta".into(),
                    serde_json::to_value(m).unwrap_or(serde_json::Value::Null),
                );
            }
        }
        emit_json_line(&payload);
    } else {
        // Bare rows on stdout (composable); human summary + optional meta on stderr.
        // A serialization failure is a system error, not empty/`[]` output — never
        // pass off a truncated array as the real data.
        let out = match serde_json::to_string_pretty(&rows) {
            Ok(s) => s,
            Err(e) => exit_error(
                json,
                2,
                "serialization_failed",
                &format!("cannot serialize parsed rows: {e}"),
                None,
                None,
            ),
        };
        println!("{out}");
        eprintln!(
            "parsed {} row{} from {} ({fmt})",
            rows.len(),
            if rows.len() == 1 { "" } else { "s" },
            file.display()
        );
        if let Some(m) = &meta_val {
            let fields = m
                .fields
                .iter()
                .map(|(k, v)| format!("{k}:{v:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!("fields: {fields}");
        }
    }
}

/// Infer the data format from a file extension: `.csv` → csv, `.json` → json,
/// `.mbox`/`.eml` → mbox. Returns `None` for anything else, so the caller can ask
/// for an explicit `--format`.
fn detect_data_format(file: &Path) -> Option<&'static str> {
    match file
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase()
        .as_str()
    {
        "csv" => Some("csv"),
        "json" => Some("json"),
        "mbox" | "eml" => Some("mbox"),
        _ => None,
    }
}

/// Read a data file into memory, bounded to a 50 MB safety cap. Strict like
/// `create`: a missing path, a directory, a non-regular file, or an oversize
/// file each exits with an informative envelope rather than a silent truncation.
fn read_data_file(file: &Path, json: bool) -> Vec<u8> {
    let meta = match std::fs::metadata(file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => exit_error(
            json,
            1,
            "no_such_path",
            &format!("no such file: {}", file.display()),
            Some(&file.display().to_string()),
            None,
        ),
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    };
    if meta.is_dir() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is a directory; `data` takes a single file",
                file.display()
            ),
            Some(&file.display().to_string()),
            None,
        );
    }
    // Reject FIFOs / sockets / devices: like `create`, a named pipe reports a
    // zero length (passing the size check) but would then block `open`/read
    // forever. Only a regular file is servable.
    if !meta.is_file() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is not a regular file (FIFOs, sockets, and devices are not supported)",
                file.display()
            ),
            Some(&file.display().to_string()),
            None,
        );
    }
    let cap = glasspad::data::limits::MAX_CSV_BYTES as u64;
    if meta.len() > cap {
        exit_error(
            json,
            1,
            "file_too_large",
            &format!(
                "{} is {} bytes, over the {cap}-byte limit",
                file.display(),
                meta.len()
            ),
            None,
            None,
        );
    }
    match read_capped(file, cap) {
        Ok(b) if b.len() as u64 > cap => exit_error(
            json,
            1,
            "file_too_large",
            &format!("{} exceeds the {cap}-byte limit", file.display()),
            None,
            None,
        ),
        Ok(b) => b,
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    }
}

// --- skill (AI-first §15) -------------------------------------------------

/// `glasspad skill` — print the companion `SKILL.md`, or install it into a Claude
/// Code skills directory (`--install-claude`, `--user` for `~/.claude`).
///
/// Under `--json`, an install emits the AI-first §10 success envelope
/// (`{schema_version, installed, scope, path, created, cli_version}`); the error
/// path (e.g. no project-level `.claude/`) uses the shared [`exit_error`] contract
/// (structured error on stderr, non-zero exit). The bare, non-install `skill`
/// stays a pure content dump on stdout in both modes.
pub fn skill(install_claude: bool, user: bool, json: bool) {
    let skill_content = include_str!("skill.md");

    // `--user` requires `--install-claude` (clap-enforced), so `install_claude`
    // alone gates the install branch.
    if install_claude {
        let base = if user {
            match dirs::home_dir() {
                Some(h) => h.join(".claude"),
                // A missing home dir is a system-level failure the caller cannot
                // fix by correcting input → structured error, exit 2 (never panic,
                // which would bypass the --json contract with a raw backtrace).
                None => exit_error(
                    json,
                    2,
                    "home_dir_not_found",
                    "cannot determine home directory for a --user install ($HOME unset)",
                    None,
                    None,
                ),
            }
        } else {
            let claude_dir = PathBuf::from(".claude");
            if !claude_dir.exists() {
                exit_error(
                    json,
                    1,
                    "claude_dir_not_found",
                    ".claude/ directory not found in current directory. \
                     Are you in a project root? Use --install-claude --user for a user-level install.",
                    None,
                    None,
                );
            }
            claude_dir
        };

        let dir = base.join("skills/glasspad");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            exit_error(
                json,
                2,
                "io_error",
                &format!("cannot create {}: {e}", dir.display()),
                None,
                None,
            );
        }
        let path = dir.join("SKILL.md");
        // `created` tracks the SKILL.md file specifically (not the dir tree, which
        // may pre-exist). Decide it atomically: an exclusive create_new tells a
        // fresh install (created=true) from an in-place refresh apart from any
        // racing installer — a plain exists()+write would misreport under a race.
        use std::io::Write as _;
        let created = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                if let Err(e) = f.write_all(skill_content.as_bytes()) {
                    exit_error(
                        json,
                        2,
                        "io_error",
                        &format!("cannot write {}: {e}", path.display()),
                        None,
                        None,
                    );
                }
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if let Err(e) = std::fs::write(&path, skill_content) {
                    exit_error(
                        json,
                        2,
                        "io_error",
                        &format!("cannot write {}: {e}", path.display()),
                        None,
                        None,
                    );
                }
                false
            }
            Err(e) => exit_error(
                json,
                2,
                "io_error",
                &format!("cannot write {}: {e}", path.display()),
                None,
                None,
            ),
        };

        if json {
            // Prefer the canonical absolute path (the file now exists, so this
            // resolves), falling back to the join if canonicalization fails.
            let resolved = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            let payload = json!({
                "schema_version": SCHEMA_VERSION,
                "installed": true,
                "scope": if user { "user" } else { "project" },
                "path": resolved.display().to_string(),
                "created": created,
                "cli_version": env!("CARGO_PKG_VERSION"),
                // Present (empty) for cross-command uniformity: callers read
                // `warnings` unconditionally across every envelope (see `data`).
                "warnings": [],
            });
            emit_json_line(&payload);
        } else {
            println!("Installed skill to {}", path.display());
        }
    } else {
        print!("{skill_content}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_kind_matches_wrap_classifier() {
        // `create` reports the same authoring level the content route acts on.
        assert!(wrap::is_fragment("<h1>hi</h1>"));
        assert!(!wrap::is_fragment("<!doctype html><html></html>"));
        // BOM + whitespace + leading comment before a real doctype → full document.
        assert!(!wrap::is_fragment(
            "\u{feff}  <!-- x -->\n<!DOCTYPE HTML><html>…"
        ));
    }

    #[test]
    fn one_artifact_snapshot_home_and_title() {
        let snap = server::one_artifact_snapshot("report", "<title>Q3</title><h1>x</h1>".into());
        let sp = snap.space("report").unwrap();
        assert_eq!(sp.home.as_deref(), Some(server::SINGLE_SLUG));
        assert_eq!(sp.nav, vec![server::SINGLE_SLUG.to_string()]);
        assert_eq!(sp.artifact(server::SINGLE_SLUG).unwrap().title, "Q3");
    }

    #[test]
    fn one_artifact_title_falls_back_to_space_name() {
        let snap = server::one_artifact_snapshot("myspace", "<p>no title here</p>".into());
        assert_eq!(
            snap.space("myspace")
                .unwrap()
                .artifact(server::SINGLE_SLUG)
                .unwrap()
                .title,
            "myspace"
        );
    }

    #[test]
    fn data_format_inferred_from_extension() {
        assert_eq!(detect_data_format(Path::new("x.csv")), Some("csv"));
        assert_eq!(detect_data_format(Path::new("x.JSON")), Some("json")); // case-insensitive
        assert_eq!(detect_data_format(Path::new("mail.mbox")), Some("mbox"));
        assert_eq!(detect_data_format(Path::new("one.eml")), Some("mbox"));
        assert_eq!(detect_data_format(Path::new("notes.txt")), None);
        assert_eq!(detect_data_format(Path::new("noext")), None);
    }

    #[test]
    fn cli_and_router_share_one_space_grammar() {
        // The names `open`/`create` accept are exactly what the router serves.
        assert!(artifact_host::valid_space("sales-q3"));
        assert!(!artifact_host::valid_space("api")); // reserved
        assert!(!artifact_host::valid_space("Bad_Name")); // grammar
        assert!(!artifact_host::valid_space("")); // empty
    }
}
