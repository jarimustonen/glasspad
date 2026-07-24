//! The `glasspad` CLI surface (Wave 3a / Phase 3): `serve`, `create`, `open`.
//!
//! Follows the project's AI-first CLI conventions (`AGENTS-AI-FIRST-CLI.md`):
//! strict input validation with informative, actionable errors; a stable,
//! versioned `--json` envelope on every command; errors as a structured envelope
//! on **stderr** with a meaningful exit code (1 = user error, 2 = system error);
//! and no interactive prompts. Paths are plain positional args — no hidden global
//! state, so the commands compose.
//!
//! The three commands are two entry points into one server plus a browser opener:
//! * `serve <dir>` drives Phase 2 live directory serving (scan + watch + SSE).
//! * `create <file>` builds a one-artifact space from a single file and serves it
//!   live (its own single-file watch).
//! * `open <space>` opens a served space's URL in the browser.
//!
//! Fragment-vs-full-document detection is **not** re-implemented here: the content
//! route classifies each artifact at serve time (`artifact_host::wrap`), so a
//! file authored either way — full `<!doctype html>` document or bare fragment —
//! is served correctly whether it arrives via `serve` or `create`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use crate::artifact_host::space::{self, ScanError};
use crate::artifact_host::{self, wrap, ArtifactHost};
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
        eprintln!("{}", serde_json::to_string(&payload).unwrap_or_default());
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

/// The comma-joined reserved-name list, for error hints.
fn reserved_list() -> Vec<String> {
    artifact_host::RESERVED.iter().map(|s| s.to_string()).collect()
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
    server::serve_on(listener, app).await;
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
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
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
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
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
    server::serve_on(listener, app).await;
}

/// Validate + read the single file `create` serves, resolving the space name.
/// Strict (AI-first §1): a missing path, a directory, a non-regular / oversize /
/// non-UTF-8 file, or an un-derivable/invalid space name each exits with an
/// informative envelope rather than a silent fixup. Returns `(space_name, html)`.
fn load_single_file(file: &Path, name_override: Option<&str>, json: bool) -> (String, String) {
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

    let bytes = match std::fs::read(file) {
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
    // Re-check the actual bytes read (a concurrent write could have grown it past
    // the stat length between the check above and the read).
    if bytes.len() as u64 > space::MAX_FILE_BYTES {
        exit_error(
            json,
            1,
            "file_too_large",
            &format!(
                "{} is {} bytes, over the {}-byte per-file limit",
                file.display(),
                bytes.len(),
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

    // Space name: `--name` override, else the file stem. Same grammar the router
    // and scanner enforce, so `create` can never mint a name they would reject.
    let derived;
    let (from_flag, raw_name) = match name_override {
        Some(n) => (true, n),
        None => {
            derived = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            (false, derived.as_str())
        }
    };
    if !artifact_host::valid_space(raw_name) {
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
        exit_error(json, 1, "invalid_space_name", &message, Some(raw_name), Some(reserved_list()));
    }

    (raw_name.to_string(), html)
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
        println!("{}", serde_json::to_string(&payload).unwrap_or_default());
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
            Some(reserved_list()),
        );
    }
    let url = format!("http://127.0.0.1:{port}/{space}/");
    let launched = if no_browser { false } else { launch_browser(&url) };

    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "space": space,
            "port": port,
            "url": url,
            "browser_launched": launched,
            "warnings": [],
        });
        println!("{}", serde_json::to_string(&payload).unwrap_or_default());
    } else if launched {
        println!("Opening {url}");
    } else {
        // Pipe-friendly: the bare URL on stdout so `open --no-browser` composes.
        println!("{url}");
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

// --- skill (AI-first §15) -------------------------------------------------

/// `glasspad skill` — print the companion `SKILL.md`, or install it into a Claude
/// Code skills directory (`--install-claude`, `--user` for `~/.claude`).
pub fn skill(install_claude: bool, user: bool) {
    let skill_content = include_str!("skill.md");

    if install_claude || user {
        let base = if user {
            dirs::home_dir()
                .expect("Cannot determine home directory")
                .join(".claude")
        } else {
            let claude_dir = PathBuf::from(".claude");
            if !claude_dir.exists() {
                eprintln!("error: .claude/ directory not found in current directory");
                eprintln!("Are you in a project root? Use --user for user-level install.");
                std::process::exit(1);
            }
            claude_dir
        };

        let dir = base.join("skills/glasspad");
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
            eprintln!("error: creating directory: {e}");
            std::process::exit(2);
        });
        let path = dir.join("SKILL.md");
        std::fs::write(&path, skill_content).unwrap_or_else(|e| {
            eprintln!("error: writing skill: {e}");
            std::process::exit(2);
        });
        println!("Installed skill to {}", path.display());
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
        assert!(!wrap::is_fragment("\u{feff}  <!-- x -->\n<!DOCTYPE HTML><html>…"));
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
            snap.space("myspace").unwrap().artifact(server::SINGLE_SLUG).unwrap().title,
            "myspace"
        );
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
