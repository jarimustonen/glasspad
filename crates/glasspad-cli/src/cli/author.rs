use super::publish::{resolve_favicon, resolve_favicon_lenient};
use super::runtime::*;
use super::serve::{bind_all_or_exit, lan_url, loopback_host, warn_lan_exposure};
use super::*;

// --- create ---------------------------------------------------------------

/// `glasspad create <file> [--name <space>]` — build a one-artifact space from a
/// single file and serve it live (a single-file watch reloads on edit). The space
/// name defaults to the file stem (validated) and can be overridden with `--name`.
pub async fn create(
    file: PathBuf,
    name: Option<String>,
    port: u16,
    lan: Option<server::LanExposure>,
    open: bool,
    json: bool,
) {
    let (space_name, html) = load_single_file(&file, name.as_deref(), json);
    // Report which authoring level was detected — the same classifier the content
    // route uses to decide wrap-vs-verbatim (design.md §4 / plan §4).
    let kind = if wrap::is_fragment(&html) {
        "fragment"
    } else {
        "full-document"
    };

    let host = loopback_host(
        port,
        resolve_favicon_lenient(),
        lan.as_ref().map(|l| l.origin.clone()),
    );
    host.swap(server::one_artifact_snapshot(&space_name, html));

    let listeners = bind_all_or_exit(port, lan.as_ref(), json).await;

    let pid_warnings = acquire_pidfile(json).await;
    server::spawn_file_watcher(host.clone(), file, space_name.clone());
    let url_path = format!("/{space_name}/");
    if let Some(l) = lan.as_ref() {
        warn_lan_exposure(l, &url_path);
    }
    emit_created(json, port, &space_name, kind, lan.as_ref(), pid_warnings);
    if open {
        let _ = launch_browser(&format!("http://127.0.0.1:{port}{url_path}"));
    }

    let policy = match lan.as_ref() {
        Some(l) => HostPolicy {
            port,
            allow_host: Some(l.allow_host.clone()),
        },
        None => HostPolicy::loopback(port),
    };
    let app = server::build_app_with_host(policy, host);
    if let Err(e) = server::serve_on_all(listeners, app).await {
        pidfile::remove_if_owned(std::process::id());
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
pub(super) fn load_single_file(
    file: &Path,
    name_override: Option<&str>,
    json: bool,
) -> (String, String) {
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
pub(super) fn resolve_space_name(file: &Path, name_override: Option<&str>, json: bool) -> String {
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
pub(super) fn read_capped(file: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let f = std::fs::File::open(file)?;
    let mut buf = Vec::new();
    f.take(max + 1).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Print the `create` startup envelope (mirrors [`emit_serving`], plus the single
/// slug and the detected authoring `kind`). `pid` names what `stop` targets;
/// `warnings` carries any pid-file takeover note.
pub(super) fn emit_created(
    json: bool,
    port: u16,
    space: &str,
    kind: &str,
    lan: Option<&server::LanExposure>,
    warnings: Vec<String>,
) {
    let url = format!("http://127.0.0.1:{port}/{space}/");
    let pid = std::process::id();
    let (lan_field, lan_host_field) = match lan {
        Some(l) => (
            json!(lan_url(l, &format!("/{space}/"))),
            json!(l.allow_host),
        ),
        None => (serde_json::Value::Null, serde_json::Value::Null),
    };
    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "serving": true,
            "port": port,
            "pid": pid,
            "space": space,
            "slug": server::SINGLE_SLUG,
            "home": server::SINGLE_SLUG,
            "url": url,
            "lan": lan_field,
            "lan_host": lan_host_field,
            "kind": kind,
            "warnings": warnings,
        });
        emit_json_line(&payload);
    } else {
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        eprintln!("glasspad serving '{space}' ({kind}) at {url} (pid {pid})");
    }
}

// --- render (markdown + reusable template) --------------------------------

/// `glasspad render <markdown-file> [--template <ref>] [--name <space>]` — render
/// a markdown body through a referenced reusable template into a hosted artifact
/// and serve it live (a re-render on every edit of the markdown — or, for a file
/// template, of the template — reloads the browser).
///
/// The template governs **only the artifact body** (`markdown-template-render`
/// decided model): it is spliced into the body via the same content-route seam a
/// `create`d fragment uses (`wrap::render_artifact` → `base.css` + `bridge.js`
/// under the frozen artifact CSP), so it can never touch the trusted shell, CSP,
/// Trusted Types, nav, or the sandbox. See `render` module docs for the boundary
/// argument.
///
/// Strict validation + a stable `--json` envelope, per AGENTS-AI-FIRST-CLI.md.
pub async fn render(
    file: PathBuf,
    template_ref: Option<String>,
    name: Option<String>,
    port: u16,
    lan: Option<server::LanExposure>,
    open: bool,
    json: bool,
) {
    // Validate the space name FIRST (fail-fast §1): it comes from `--name` or the
    // markdown file stem, neither of which needs file contents.
    let space_name = resolve_space_name(&file, name.as_deref(), json);

    // Read + validate the markdown source (same strict checks as `create`).
    let markdown = read_capped_utf8_file(&file, "markdown", "no_such_path", json);

    // Resolve the template reference to its source string + the watcher handle.
    let (template, template_str, kind, label) = resolve_template(template_ref.as_deref(), json);

    // Render markdown + template into the artifact body. A template that lost its
    // single `{{content}}` placeholder is a user error (§1), reported informatively.
    let body = match render::render_to_body(&markdown, &template_str) {
        Ok(b) => b,
        Err(e) => exit_error(
            json,
            1,
            "invalid_template",
            &e.to_string(),
            Some(&label),
            None,
        ),
    };
    // Bound the generated body to the same per-artifact limit `create`/`serve`
    // enforce (rendering can amplify markup past the input cap).
    let body = match server::enforce_body_cap(body) {
        Ok(b) => b,
        Err(msg) => exit_error(json, 1, "rendered_output_too_large", &msg, None, None),
    };

    // A file template that renders a FULL document (opens with `<!doctype>`/`<html>`)
    // is served verbatim — it forgoes the fragment wrap, so it loses the auto-linked
    // `base.css` (incl. the `.gp-prose` theme) and injected `bridge.js` (live reload
    // in-frame). Not a security issue (the `_c` response CSP/sandbox are unchanged),
    // but a footgun worth a non-fatal warning so the author isn't surprised.
    let mut warnings: Vec<String> = Vec::new();
    if wrap::is_full_document(&body) {
        warnings.push(
            "the template renders a full HTML document (opens with <!doctype>/<html>): \
             it is served verbatim, so glasspad does NOT link base.css (the .gp-prose \
             theme) or inject bridge.js (in-frame live reload). Use a fragment template \
             (e.g. the built-in prose/dashboard) to keep those, or link base.css yourself."
                .to_string(),
        );
    }

    let host = loopback_host(
        port,
        resolve_favicon_lenient(),
        lan.as_ref().map(|l| l.origin.clone()),
    );
    host.swap(server::one_artifact_snapshot(&space_name, body));

    let listeners = bind_all_or_exit(port, lan.as_ref(), json).await;

    warnings.extend(acquire_pidfile(json).await);
    server::spawn_render_watcher(host.clone(), file, template, space_name.clone());
    let url_path = format!("/{space_name}/");
    if let Some(l) = lan.as_ref() {
        warn_lan_exposure(l, &url_path);
    }
    emit_rendered(
        json,
        port,
        &space_name,
        &label,
        kind,
        lan.as_ref(),
        warnings,
    );
    if open {
        let _ = launch_browser(&format!("http://127.0.0.1:{port}{url_path}"));
    }

    let policy = match lan.as_ref() {
        Some(l) => HostPolicy {
            port,
            allow_host: Some(l.allow_host.clone()),
        },
        None => HostPolicy::loopback(port),
    };
    let app = server::build_app_with_host(policy, host);
    if let Err(e) = server::serve_on_all(listeners, app).await {
        pidfile::remove_if_owned(std::process::id());
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

/// Resolve `--template <ref>` (default `prose`) to `(watcher handle, source string,
/// kind, label)`. **Resolution rule:** an exact built-in name (`prose` /
/// `dashboard`) resolves to that built-in; **anything else** is a filesystem path
/// to a template file (read strictly). Built-in names contain no `/` or `.`, so a
/// local file literally named `prose` is reachable as `./prose` (≠ `"prose"` → a
/// path) — unambiguous. `kind` is `"builtin"`/`"file"` for the envelope; `label` is
/// the reference echoed back (the name or the path).
pub(super) fn resolve_template(
    template_ref: Option<&str>,
    json: bool,
) -> (RenderTemplate, String, &'static str, String) {
    let reference = template_ref.unwrap_or(render::DEFAULT_TEMPLATE);
    if let Some(builtin) = render::builtin_template(reference) {
        return (
            RenderTemplate::Builtin(builtin),
            builtin.to_string(),
            "builtin",
            reference.to_string(),
        );
    }
    // A filesystem path to a template file. A *bare* name (no `/`, no `.`) that is
    // neither a built-in nor an existing file is almost certainly a mistyped
    // built-in, not a path — surface the built-in allowlist rather than a bare
    // "no such file" (AI-first §10: an `expected` set on a fixed-enum-like arg).
    let path = PathBuf::from(reference);
    let looks_like_path = reference.contains('/') || reference.contains('.');
    if !looks_like_path && !path.exists() {
        exit_error(
            json,
            1,
            "unknown_template",
            &format!(
                "unknown template {reference:?}: expected a built-in ({}) or a path to a \
                 template file (e.g. ./my-template.html)",
                render::BUILTIN_NAMES.join(", ")
            ),
            Some(reference),
            Some(
                render::BUILTIN_NAMES
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
        );
    }
    let content = read_capped_utf8_file(&path, "template", "template_not_found", json);
    (
        RenderTemplate::File(path),
        content,
        "file",
        reference.to_string(),
    )
}

/// Read + validate a UTF-8 source file (markdown or template), bounded to the
/// per-file cap. Strict like `create` (fail-fast §1): a missing path, a directory,
/// a non-regular / oversize / non-UTF-8 file each exits with an informative
/// envelope rather than a silent fixup. `noun` names the file kind in messages;
/// `missing_code` is the stable `code` for a not-found path (so a missing template
/// reports `template_not_found`, a missing markdown `no_such_path`).
pub(super) fn read_capped_utf8_file(
    file: &Path,
    noun: &str,
    missing_code: &str,
    json: bool,
) -> String {
    let meta = match std::fs::metadata(file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => exit_error(
            json,
            1,
            missing_code,
            &format!("no such {noun} file: {}", file.display()),
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
                "{} is a directory; a {noun} must be a single file",
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
                "{} is not a regular file (FIFOs, sockets, and devices are not supported)",
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
    match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => exit_error(
            json,
            1,
            "not_utf8",
            &format!(
                "{} is not valid UTF-8 ({noun} must be UTF-8)",
                file.display()
            ),
            None,
            None,
        ),
    }
}

/// Print the `render` startup envelope (mirrors [`emit_created`], plus the resolved
/// template + its kind, and any non-fatal `warnings`).
pub(super) fn emit_rendered(
    json: bool,
    port: u16,
    space: &str,
    template: &str,
    kind: &str,
    lan: Option<&server::LanExposure>,
    warnings: Vec<String>,
) {
    let url = format!("http://127.0.0.1:{port}/{space}/");
    let pid = std::process::id();
    let (lan_field, lan_host_field) = match lan {
        Some(l) => (
            json!(lan_url(l, &format!("/{space}/"))),
            json!(l.allow_host),
        ),
        None => (serde_json::Value::Null, serde_json::Value::Null),
    };
    if json {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "serving": true,
            "port": port,
            "pid": pid,
            "space": space,
            "slug": server::SINGLE_SLUG,
            "home": server::SINGLE_SLUG,
            "url": url,
            "lan": lan_field,
            "lan_host": lan_host_field,
            "template": template,
            "template_kind": kind,
            "warnings": warnings,
        });
        emit_json_line(&payload);
    } else {
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        eprintln!(
            "glasspad serving '{space}' (rendered via {kind} template '{template}') at {url} \
             (pid {pid})"
        );
    }
}

// --- build (static render) ------------------------------------------------

/// `glasspad build <space> <out> [--shared-libs] [--force] [--dry-run]` —
/// statically render a space directory to self-contained HTML files (no server,
/// no bind). Reuses the same security-checked scanner + wrap seam `serve` uses,
/// producing the same wrapped pages the content route would serve, written to
/// `<out>` for an offline docsite / external preview transport (see `build` docs).
///
/// Strict + fail-fast (AI-first §1): a symlink / traversal / reserved-slug /
/// oversize input is refused by the scanner before anything is written, and a
/// non-empty `<out>` is refused unless `--force` (§3 — a potentially-overwriting
/// write opts in explicitly). `--dry-run` (§11) validates + plans and prints the
/// file list without touching the filesystem.
pub fn build(
    space_dir: PathBuf,
    out: PathBuf,
    shared_libs: bool,
    force: bool,
    dry_run: bool,
    json: bool,
) {
    let mode = if shared_libs {
        LibMode::SharedLibs
    } else {
        LibMode::SelfContained
    };

    // Scan the space with the SAME scanner `serve` uses: a symlink, path
    // traversal, reserved slug, collision, or oversize file is refused here just
    // as on the server path (AI-first §1), before any output is written.
    let (name, snap) = match server::scan_named(&space_dir) {
        Ok(x) => x,
        Err(e) => exit_scan_error(&e, json),
    };
    let space = snap.space(&name).expect("scanned space is present");
    let home = space.home.clone();
    let slugs = space.slugs();

    // Refuse an output that would overwrite or pollute the source space: writing
    // INTO (or AT) the scanned directory would either clobber source files or seed
    // the next scan with generated `.html`/`_gp` output. Checked before planning so
    // it surfaces in --dry-run too (a read-only, non-mutating validation, §11).
    guard_out_not_in_space(&space_dir, &out, json);

    // Resolve the repo favicon (`.glasspad.yaml`) for the built pages' outer <head>;
    // an invalid emoji is a hard error here, before any output is written (§1).
    let favicon = resolve_favicon(json);

    // Plan every output file (pure — no filesystem writes yet).
    let files = build::plan(space, home.as_deref(), mode, favicon.as_deref());
    let index = files
        .iter()
        .any(|f| f.rel_path == "index.html")
        .then(|| "index.html".to_string());

    // Non-fatal caveats every build carries (AI-first §10 warnings go in the
    // stdout payload / on stderr in text mode). The security note is standing: the
    // static output is NOT the live host's sandbox.
    let mut warnings: Vec<String> = vec![
        "static output is NOT sandboxed like the live host (no null-origin iframe, no \
         per-response CSP) and has no trusted nav shell: cross-artifact bridge navigation \
         and extensionless relative links (href=\"other-slug\") do not resolve — link with \
         an explicit .html. Build only spaces you trust; serve the output at a web root \
         (or open index.html) so the base libs resolve."
            .to_string(),
    ];
    if slugs.is_empty() {
        warnings
            .push("the space contains no artifacts: the build produced no entry page.".to_string());
    }

    // Validate the output directory (read-only: metadata + read_dir). Done for BOTH
    // dry-run and the real run so --dry-run performs the same non-mutating checks
    // the real run does (§11). Pass --force to preview/allow a non-empty target.
    guard_out_dir(&out, force, json);

    if dry_run {
        emit_build_report(
            json,
            true,
            &out,
            &name,
            mode,
            &slugs,
            &files,
            home.as_deref(),
            index.as_deref(),
            &warnings,
        );
        return;
    }

    if let Err(e) = build::write_files(&out, &files) {
        exit_error(
            json,
            2,
            "io_error",
            &format!("cannot write build output under {}: {e}", out.display()),
            None,
            None,
        );
    }

    // Prefer the canonical absolute path now that the directory exists.
    let resolved = std::fs::canonicalize(&out).unwrap_or_else(|_| out.clone());
    emit_build_report(
        json,
        false,
        &resolved,
        &name,
        mode,
        &slugs,
        &files,
        home.as_deref(),
        index.as_deref(),
        &warnings,
    );
}

/// Reject an output directory that equals or is nested inside the source space.
/// Both are resolved to absolute paths (`out` via its nearest existing ancestor,
/// since it need not exist yet) so the check is robust to `.`/`..`/symlink
/// spellings. This is independent of `--force`: writing at/into the source would
/// overwrite artifacts or seed the next scan with generated output.
pub(super) fn guard_out_not_in_space(space_dir: &Path, out: &Path, json: bool) {
    let space_abs = std::fs::canonicalize(space_dir).unwrap_or_else(|_| space_dir.to_path_buf());
    let out_abs = abs_via_nearest_ancestor(out);
    if out_abs == space_abs || out_abs.starts_with(&space_abs) {
        exit_error(
            json,
            1,
            "output_inside_space",
            &format!(
                "output {} is the source space {} (or nested inside it); choose an output \
                 directory outside the space so the build cannot overwrite or re-scan its own output",
                out.display(),
                space_dir.display()
            ),
            Some(&out.display().to_string()),
            None,
        );
    }
}

/// Resolve `path` to an absolute path even when it does not exist yet:
/// canonicalize the deepest existing ancestor and re-append the non-existent tail.
/// Used to compare a not-yet-created output dir against the (existing) source root.
pub(super) fn abs_via_nearest_ancestor(path: &Path) -> PathBuf {
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = path.to_path_buf();
    loop {
        if let Ok(canon) = std::fs::canonicalize(&cur) {
            let mut result = canon;
            for seg in tail.iter().rev() {
                result.push(seg);
            }
            return result;
        }
        match cur.file_name() {
            Some(name) => tail.push(name.to_os_string()),
            None => break,
        }
        if !cur.pop() {
            break;
        }
    }
    // Nothing on the path existed (or a rootless relative path): best-effort absolute.
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Refuse to write into an existing non-empty `<out>` unless `--force`. A path that
/// does not exist is fine (`write_files` creates it); a path that exists but is not
/// a directory is always an error. IO errors reading the directory are system
/// errors (exit 2).
pub(super) fn guard_out_dir(out: &Path, force: bool, json: bool) {
    match std::fs::metadata(out) {
        Ok(m) if m.is_dir() => {
            if !force {
                // A read failure here is a SYSTEM error (exit 2), not "empty"; a
                // first entry that errors is likewise an I/O failure, not "non-empty".
                let mut it = match std::fs::read_dir(out) {
                    Ok(it) => it,
                    Err(e) => exit_error(
                        json,
                        2,
                        "io_error",
                        &format!("cannot read output directory {}: {e}", out.display()),
                        None,
                        None,
                    ),
                };
                match it.next() {
                    None => {} // empty → fine
                    Some(Err(e)) => exit_error(
                        json,
                        2,
                        "io_error",
                        &format!("cannot read output directory {}: {e}", out.display()),
                        None,
                        None,
                    ),
                    Some(Ok(_)) => exit_error(
                        json,
                        1,
                        "output_not_empty",
                        &format!(
                            "output directory {} is not empty; pass --force to write into it \
                             (existing files may be overwritten)",
                            out.display()
                        ),
                        Some(&out.display().to_string()),
                        None,
                    ),
                }
            }
        }
        Ok(_) => exit_error(
            json,
            1,
            "output_not_a_directory",
            &format!(
                "{} exists and is not a directory; give a directory path for the build output",
                out.display()
            ),
            Some(&out.display().to_string()),
            None,
        ),
        // Absent: created by `write_files`.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot access {}: {e}", out.display()),
            None,
            None,
        ),
    }
}

/// Emit the `build` result (`dry` = false) or dry-run plan (`dry` = true). The
/// dry-run form carries the AI-first §11 `would[]` planning list and `dry_run:
/// true`; the real-run form reports `built: true` and the written counts. Both
/// share the descriptive fields so a caller reads the same shape either way.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_build_report(
    json: bool,
    dry: bool,
    out: &Path,
    name: &str,
    mode: LibMode,
    slugs: &[String],
    files: &[build::OutFile],
    home: Option<&str>,
    index: Option<&str>,
    warnings: &[String],
) {
    if json {
        // `built`/`dry_run` are both present in every payload (one true, one false)
        // so an AI consumer reads a stable shape without mode-dependent field probing.
        let mut payload = json!({
            "schema_version": SCHEMA_VERSION,
            "built": !dry,
            "dry_run": dry,
            "space": name,
            "out": out.display().to_string(),
            "mode": mode.as_str(),
            "home": home,
            "index": index,
            "artifacts": slugs,
            "pages": slugs.len(),
            "files": files.len(),
            "base_libs_bundled": mode == LibMode::SelfContained,
            "warnings": warnings,
        });
        if dry {
            let would: Vec<serde_json::Value> = files
                .iter()
                .map(|f| {
                    json!({
                        "action": "write",
                        "resource": "file",
                        "path": f.rel_path,
                        "bytes": f.bytes.len(),
                    })
                })
                .collect();
            payload
                .as_object_mut()
                .expect("object literal")
                .insert("would".into(), json!(would));
        }
        emit_json_line(&payload);
    } else if dry {
        for w in warnings {
            eprintln!("warning: {w}");
        }
        eprintln!(
            "glasspad build (dry run): would write {} file(s) for space '{name}' ({}) to {}",
            files.len(),
            mode.as_str(),
            out.display()
        );
        for f in files {
            eprintln!("  {} ({} bytes)", f.rel_path, f.bytes.len());
        }
    } else {
        for w in warnings {
            eprintln!("warning: {w}");
        }
        // Bare output path on stdout (composable); human summary on stderr.
        println!("{}", out.display());
        eprintln!(
            "glasspad built space '{name}' ({}) to {} — {} page(s), {} file(s)",
            mode.as_str(),
            out.display(),
            slugs.len(),
            files.len()
        );
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
pub(super) fn launch_browser(url: &str) -> bool {
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
