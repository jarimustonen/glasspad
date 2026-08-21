use super::author::{
    create, launch_browser, read_capped, read_capped_utf8_file, render, resolve_template,
};
use super::runtime::*;
use super::serve::serve;
use super::*;

// --- publish (the default verb) -------------------------------------------

/// What a `publish` / `loopback serve` `<path>` resolves to. A directory is an
/// N-page space; a single `.md`/`.markdown` file is a one-page space rendered
/// through a template; a single `.html`/`.htm` file is a one-page space served
/// verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PathKind {
    Dir,
    Markdown,
    Html,
}

/// Classify a `<path>` as a directory or a supported single file (strict, AI-first
/// §1): a missing path, a non-regular file, or an unsupported extension each exit
/// with an informative envelope rather than a silent fixup. `metadata` follows a
/// symlink — the caller named this path explicitly (unlike a directory *scan*,
/// where a symlinked entry can smuggle a file in from outside the space).
pub(super) fn classify_publish_path(path: &Path, json: bool) -> PathKind {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => exit_error(
            json,
            1,
            "no_such_path",
            &format!("no such file or directory: {}", path.display()),
            Some(&path.display().to_string()),
            None,
        ),
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", path.display()),
            None,
            None,
        ),
    };
    if meta.is_dir() {
        return PathKind::Dir;
    }
    if !meta.is_file() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is not a regular file or directory (FIFOs, sockets, and devices are not publishable)",
                path.display()
            ),
            None,
            None,
        );
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("md") | Some("markdown") => PathKind::Markdown,
        Some("html") | Some("htm") => PathKind::Html,
        _ => exit_error(
            json,
            1,
            "unsupported_input",
            &format!(
                "{} is not a publishable file: pass a .md/.markdown/.html file or a directory of them",
                path.display()
            ),
            Some(&path.display().to_string()),
            Some(vec![
                "md".into(),
                "markdown".into(),
                "html".into(),
                "htm".into(),
            ]),
        ),
    }
}

/// `glasspad publish <path>` — THE default verb. Resolve the publish target from
/// config (per-key merge of repo-local `.glasspad.yaml` → home config → the
/// built-in `loopback` default, with a `--target`/`$GLASSPAD_TARGET` override), then
/// dispatch: `loopback` serves the space live on 127.0.0.1 (folding
/// serve/create/render/open), `hosted` uploads it and returns a `/p/<slug>/` URL.
///
/// `<path>` is a `.md`/`.markdown`/`.html` file (a one-page space) or a directory
/// of them (an N-page space); markdown is rendered automatically via the resolved
/// template (flag > config `template:` > built-in `prose`).
#[allow(clippy::too_many_arguments)]
pub async fn publish(
    path: PathBuf,
    target: Option<String>,
    server: Option<String>,
    api_key: Option<String>,
    template: Option<String>,
    title: Option<String>,
    space_key: Option<String>,
    update: Option<String>,
    port: Option<u16>,
    no_open: bool,
    json: bool,
) {
    let cfg = resolve_publish_config(json);
    let resolved_target = resolve_target(target, &cfg, json);
    let kind = classify_publish_path(&path, json);

    // An explicit `--template` applies ONLY to a single markdown file (never silently
    // ignored): a directory's markdown pages take their template from the space's
    // `glasspad.yaml`, and `.html` is published verbatim.
    if template.is_some() && kind != PathKind::Markdown {
        exit_error(
            json,
            1,
            "template_not_applicable",
            "--template only applies to a single markdown file: a directory's markdown pages take \
             their template from the space's glasspad.yaml, and .html is published verbatim",
            None,
            None,
        );
    }
    // The template default is a fallback for a single markdown file only, so a
    // repo-wide default never breaks publishing `.html` or a directory. Precedence
    // mirrors the other settings — flag > $GLASSPAD_TEMPLATE > config `template:` —
    // and the built-in `prose` default is applied downstream by `resolve_template`.
    let md_template = if kind == PathKind::Markdown {
        resolve_setting(template, "GLASSPAD_TEMPLATE", cfg.template.clone())
    } else {
        None
    };

    match resolved_target {
        Target::Loopback => {
            // Hosted-only options on a loopback target are a usage error, not a
            // silent no-op (AI-first strict validation).
            reject_hosted_flags_on_loopback(&server, &api_key, &title, &space_key, &update, json);
            let port = resolve_port(port, json);
            publish_loopback(path, kind, md_template, port, !no_open, json).await;
        }
        Target::Hosted => {
            publish_hosted(
                path,
                kind,
                server,
                api_key,
                md_template,
                title,
                space_key,
                update,
                &cfg,
                no_open,
                json,
            )
            .await;
        }
    }
}

/// Reject hosted-only options passed with a resolved loopback target, naming the
/// offending flag(s) rather than silently ignoring them.
pub(super) fn reject_hosted_flags_on_loopback(
    server: &Option<String>,
    api_key: &Option<String>,
    title: &Option<String>,
    space_key: &Option<String>,
    update: &Option<String>,
    json: bool,
) {
    let mut offenders = Vec::new();
    if server.is_some() {
        offenders.push("--server");
    }
    if api_key.is_some() {
        offenders.push("--api-key");
    }
    if title.is_some() {
        offenders.push("--title");
    }
    if space_key.is_some() {
        offenders.push("--space-key");
    }
    if update.is_some() {
        offenders.push("--update");
    }
    if !offenders.is_empty() {
        exit_error(
            json,
            1,
            "option_not_applicable",
            &format!(
                "{} {} hosted-only, but the resolved target is loopback; drop {} or set `target: \
                 hosted` (config / --target hosted)",
                offenders.join(", "),
                if offenders.len() == 1 { "is" } else { "are" },
                if offenders.len() == 1 { "it" } else { "them" },
            ),
            None,
            None,
        );
    }
}

/// The loopback branch of `publish`: serve the space live on 127.0.0.1 (blocking,
/// with live reload) and open the browser. This is the fold of the old
/// serve/create/render + open into the default path; the loopback↔hosted asymmetry
/// (live vs. snapshot) is intended (design decision #1).
pub(super) async fn publish_loopback(
    path: PathBuf,
    kind: PathKind,
    template: Option<String>,
    port: u16,
    open: bool,
    json: bool,
) {
    // `template` is `None` for a non-markdown path (validated + narrowed upstream in
    // `publish`), so the Html/Dir arms carry no template.
    // The default `publish` verb stays loopback-only (LAN reach is the explicit
    // `loopback serve --bind` opt-in), so no LAN exposure is threaded here.
    match kind {
        PathKind::Dir => serve(Some(path), port, None, open, json).await,
        PathKind::Markdown => render(path, template, None, port, None, open, json).await,
        PathKind::Html => create(path, None, port, None, open, json).await,
    }
}

/// The hosted branch of `publish`: shape the `<path>` into a space (a directory is
/// scanned; a single file is a one-page space, markdown rendered locally), then
/// upload it as one bundle to `POST /api/v1/spaces` and return the `/p/<slug>/` URL.
/// Idempotent via `space_key` (a re-publish updates the space in place).
#[allow(clippy::too_many_arguments)]
pub(super) async fn publish_hosted(
    path: PathBuf,
    kind: PathKind,
    server: Option<String>,
    api_key: Option<String>,
    template: Option<String>,
    title: Option<String>,
    space_key: Option<String>,
    update: Option<String>,
    cfg: &config::ResolvedConfig,
    no_open: bool,
    json: bool,
) {
    // `--update <slug>` targets an EXISTING space by its capability slug — a
    // per-invocation target, not a persistent identity, so it is flag-only (never
    // config/env). Validate it against the SAME slug grammar the server enforces, and
    // do it FIRST (before server/key resolution) so a malformed value is a deterministic
    // local `invalid_update_slug`, never a misleading `missing_server` or a surprising
    // URL path/query/fragment interpolated into the request (AI-first strict validation).
    let update = match &update {
        None => None,
        Some(raw) => {
            let slug = raw.trim();
            if !crate::artifact_host::valid_space(slug) {
                exit_error(
                    json,
                    1,
                    "invalid_update_slug",
                    "--update requires a valid capability slug (the <slug> in /p/<slug>/): \
                     lowercase [a-z0-9-], starting alphanumeric, at most 64 chars, not reserved",
                    Some(slug),
                    None,
                );
            }
            Some(slug.to_string())
        }
    };

    // Capture credential provenance BEFORE resolving (which consumes the flags): a
    // server/key that comes from an explicit flag or env var is user-directed and
    // safe; one that comes from config is subject to the cross-trust check below.
    let env_present = |name: &str| {
        std::env::var(name)
            .ok()
            .is_some_and(|v| !v.trim().is_empty())
    };
    let server_from_flag_or_env = server
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty())
        || env_present("GLASSPAD_SERVER");
    let api_key_from_flag_or_env = api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty())
        || env_present("GLASSPAD_API_KEY");

    let server = resolve_server(server, cfg, json);
    let api_key = resolve_api_key(api_key, cfg, json);
    // `space_key`: flag > $GLASSPAD_SPACE_KEY > config `space_key:`.
    let space_key = resolve_setting(space_key, "GLASSPAD_SPACE_KEY", cfg.space_key.clone());

    // `--update` supersedes any resolved `space_key`: naming a slug to replace means
    // addressing by URL, not by the keyed create-or-update mapping. clap already rejects
    // `--update` + an explicit `--space-key`; here we additionally drop a config/env
    // `space_key` so the two addressing modes never mix on the wire.
    if update.is_some() && space_key.is_some() {
        eprintln!(
            "note: --update names the target space directly, so the configured space_key is \
             ignored for this publish"
        );
    }
    let space_key = if update.is_some() { None } else { space_key };

    // Cross-trust credential guard (defense-in-depth): warn loudly when a hosted
    // publish would send an API key that came from the home config / environment to a
    // server whose URL came from the repo-local `.glasspad.yaml`. A cloned/untrusted
    // repository could otherwise redirect your credential to an attacker's host. The
    // safe cases stay silent: an explicit `--server`/`$GLASSPAD_SERVER`, or a key that
    // also comes from the same repo config, does not trip it. (The deeper fix — binding
    // server+key into one trusted profile — is deferred to `hosted-multiworker-credentials`.)
    let server_from_repo =
        !server_from_flag_or_env && cfg.server_origin == Some(config::Origin::Repo);
    let key_from_repo =
        !api_key_from_flag_or_env && cfg.api_key_origin == Some(config::Origin::Repo);
    if server_from_repo && !key_from_repo {
        let key_src = if api_key_from_flag_or_env {
            "the command line / environment"
        } else {
            "your home config"
        };
        eprintln!(
            "warning: publishing to {server}, whose URL comes from this repository's \
             .glasspad.yaml, using an API key from {key_src}. A cloned or untrusted repository \
             can redirect your credential to an arbitrary server this way. Pass --server / \
             --api-key explicitly, or move `server` into your home config, to confirm this is \
             intended."
        );
    }

    let mut space = match kind {
        PathKind::Dir => match space::scan_dir(&path) {
            Ok(sp) => sp,
            Err(e) => exit_error(
                json,
                1,
                "invalid_space",
                &format!("cannot publish {}: {e}", path.display()),
                None,
                None,
            ),
        },
        // `template` is `None` for a non-markdown path (validated upstream).
        PathKind::Markdown => build_single_page_space(&path, template, true, json),
        PathKind::Html => build_single_page_space(&path, None, false, json),
    };
    // Attach the validated repo favicon (`.glasspad.yaml`) so it travels in the
    // upload bundle and the hosted server renders it into the space's outer shell
    // (per-space — one hosted server carries many repos' favicons). The server
    // re-validates it at ingest; this early check surfaces a bad emoji before upload.
    space.favicon = validate_favicon(cfg.favicon.as_deref(), json);
    if space.artifacts.is_empty() {
        exit_error(
            json,
            1,
            "empty_space",
            &format!(
                "{} has no pages to publish (a space is a directory of .html and/or .md files)",
                path.display()
            ),
            None,
            None,
        );
    }

    post_space_bundle(
        &space, &server, &api_key, space_key, update, title, no_open, json,
    )
    .await;
}

/// Build a one-page [`space::Space`] from a single file — the hosted analogue of the
/// loopback `create`/`render` path. A markdown file is rendered through the resolved
/// template (flag/default → `resolve_template`) into the artifact body; an HTML file
/// is taken verbatim. The lone artifact is the space home (`SINGLE_SLUG`); its title
/// is resolved from the content (falling back to the file stem).
pub(super) fn build_single_page_space(
    file: &Path,
    template_ref: Option<String>,
    is_markdown: bool,
    json: bool,
) -> space::Space {
    let html = if is_markdown {
        let (tmpl, _src, _kind, label) = resolve_template(template_ref.as_deref(), json);
        match server::build_render_body(file, &tmpl) {
            Ok(b) => b,
            Err(msg) => exit_error(
                json,
                1,
                "render_failed",
                &format!("cannot render {}: {msg}", file.display()),
                Some(&label),
                None,
            ),
        }
    } else {
        read_capped_utf8_file(file, "html", "no_such_path", json)
    };
    // The space name only feeds the title fallback here (hosted assigns the slug),
    // so a non-grammatical stem is fine — default to "page" when there is none.
    let name = file
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("page");
    // Take the single space `one_artifact_snapshot` builds without depending on `name`
    // being a valid/normalized map key (the snapshot always contains exactly one space).
    std::sync::Arc::into_inner(
        server::one_artifact_snapshot(name, html)
            .spaces
            .into_values()
            .next()
            .expect("one_artifact_snapshot builds exactly one space"),
    )
    .expect("one_artifact_snapshot has the only Space reference")
}

/// Upload a scanned/synthesized [`space::Space`] as one bundle to
/// `POST /api/v1/spaces` and print `{slug, url}`. Shared by the directory and
/// single-file hosted publish paths (a single file is just a one-page space). A
/// `space_key` makes the publish idempotent (updates in place at the same slug);
/// `title_override` wins over the scanned space title. The API key is never printed.
///
/// When `update` is `Some(slug)`, the bundle is sent as `PUT /api/v1/spaces/<slug>`
/// instead — an in-place replace of an EXISTING space at that capability slug. That
/// path never carries `space_key` (the URL slug is the target); the caller has
/// already cleared `space_key` when `update` is set, and the two are clap-exclusive.
#[allow(clippy::too_many_arguments)]
pub(super) async fn post_space_bundle(
    space: &space::Space,
    server: &str,
    api_key: &str,
    space_key: Option<String>,
    update: Option<String>,
    title_override: Option<String>,
    no_open: bool,
    json: bool,
) {
    use base64::Engine as _;

    let pages: Vec<serde_json::Value> = space
        .artifacts
        .iter()
        .map(|(slug, art)| json!({ "slug": slug, "html": art.html }))
        .collect();
    let assets: Vec<serde_json::Value> = space
        .assets
        .iter()
        .map(|(key, asset)| {
            let rel = key.strip_prefix("assets/").unwrap_or(key);
            json!({
                "path": rel,
                "content_base64": base64::engine::general_purpose::STANDARD.encode(&asset.bytes),
            })
        })
        .collect();
    let mut body = serde_json::Map::new();
    body.insert("pages".into(), json!(pages));
    if !assets.is_empty() {
        body.insert("assets".into(), json!(assets));
    }
    if !space.nav.is_empty() {
        body.insert("nav".into(), json!(space.nav));
    }
    // Carry the grouped nav (glasspad.yaml `groups:`) so a published docsite renders
    // its grouped sidebar + landing on the hosted server. Reconciled + sanitized
    // server-side at ingest, exactly like the loopback scanner does.
    if !space.nav_groups.is_empty() {
        body.insert("groups".into(), json!(space.nav_groups));
    }
    let title = title_override.or_else(|| space.title.clone());
    if let Some(t) = &title {
        body.insert("title".into(), json!(t));
    }
    if let Some(f) = &space.favicon {
        body.insert("favicon".into(), json!(f));
    }
    // `--update <slug>` addresses by URL (PUT /api/v1/spaces/<slug>); it never carries
    // a `space_key`. The keyed create-or-update stays on POST /api/v1/spaces.
    if update.is_none()
        && let Some(k) = &space_key
    {
        body.insert("space_key".into(), json!(k));
    }

    if server.starts_with("http://") && !server_is_loopback(server) {
        eprintln!(
            "warning: publishing over plaintext http:// to a non-local host sends the API key \
             in the clear; prefer https://"
        );
    }

    let base = server.trim_end_matches('/');
    let url = match &update {
        Some(slug) => format!("{base}/api/v1/spaces/{slug}"),
        None => format!("{base}/api/v1/spaces"),
    };
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .build()
    {
        Ok(c) => c,
        Err(e) => exit_error(json, 2, "client_init_failed", &e.to_string(), None, None),
    };
    let request = if update.is_some() {
        client.put(&url)
    } else {
        client.post(&url)
    };
    let resp = request
        .bearer_auth(api_key)
        .json(&serde_json::Value::Object(body))
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => exit_error(
            json,
            2,
            "request_failed",
            &format!("cannot reach {url}: {e}"),
            None,
            None,
        ),
    };

    let status = resp.status();
    let payload: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let msg = payload
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("the server rejected the publish");
        let code = payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("publish_rejected");
        let exit = if status.is_client_error() { 1 } else { 2 };
        exit_error(
            json,
            exit,
            code,
            &format!("{msg} (HTTP {})", status.as_u16()),
            None,
            None,
        );
    }

    let slug = payload
        .get("slug")
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let space_url = payload
        .get("url")
        .and_then(|u| u.as_str())
        .filter(|u| !u.is_empty())
        .map(str::to_string);
    let (slug, space_url) = match (slug, space_url) {
        (Some(s), Some(u)) => (s, u),
        _ => exit_error(
            json,
            2,
            "malformed_response",
            &format!(
                "server returned {} but no slug/url in the body",
                status.as_u16()
            ),
            None,
            None,
        ),
    };
    let page_count = payload
        .get("page_count")
        .and_then(|c| c.as_u64())
        .unwrap_or(space.artifacts.len() as u64);
    let created = payload
        .get("created")
        .and_then(|c| c.as_bool())
        .unwrap_or(true);

    let launched = if no_open {
        false
    } else {
        launch_browser(&space_url)
    };

    // Return-channel discoverability: a hosted page's form/choice submissions are
    // delivered only to an agent actively reading them — a published-and-forgotten
    // page has no consumer, so its answers pile up unseen in the durable store. Print
    // the exact `await-submission` (block for the next answer) + `submissions` (drain
    // the backlog) invocations so a returning agent can find them, plus how long they
    // survive. Keyed off the same `server` this publish used (config/flag precedence),
    // never a hardcoded host.
    //
    // The commands are shown WITHOUT `--api-key`: both resolve the key from
    // `$GLASSPAD_API_KEY` / config (the same precedence this publish used), so the
    // printed line is copy-pasteable without pasting a secret onto argv (and where it
    // would leak into shell history / process listings). The server is single-quoted
    // so a URL is never re-interpreted by the shell. The real key is never printed.
    let server_base = server.trim_end_matches('/');
    let await_cmd = format!("glasspad await-submission --server '{server_base}' {slug}");
    let drain_cmd = format!("glasspad submissions --server '{server_base}' {slug}");
    // Only state an exact window when the server reported a sane positive value;
    // otherwise stay generic rather than print "0 day(s)".
    let retention_days = payload
        .get("retention_days")
        .and_then(|d| d.as_i64())
        .filter(|d| *d > 0);
    let retention_note = match retention_days {
        Some(d) => format!(
            "submissions are delivered only while an agent is listening; otherwise the page keeps \
             them for up to {d} day(s) (retrievable while the page is retained) — drain later with \
             `submissions`"
        ),
        None => {
            "submissions are delivered only while an agent is listening; otherwise the page keeps \
             them for its retention window (retrievable while the page is retained) — drain later \
             with `submissions`"
                .to_string()
        }
    };

    if json {
        let mut out = payload.clone();
        if let Some(obj) = out.as_object_mut() {
            obj.insert("published".into(), json!(true));
            obj.insert("browser_launched".into(), json!(launched));
            obj.insert("await_submission".into(), json!(await_cmd));
            obj.insert("drain_submissions".into(), json!(drain_cmd));
            obj.insert("submissions_note".into(), json!(retention_note));
        }
        emit_json_line(&out);
    } else {
        println!("{space_url}");
        let verb = if created { "published" } else { "updated" };
        eprintln!("{verb} space '{slug}' ({page_count} pages) to {space_url}");
        eprintln!(
            "to receive return-channel submissions from this page, run (reads \
             $GLASSPAD_API_KEY / config):"
        );
        eprintln!("  {await_cmd}");
        eprintln!("or drain what accumulated while you were away:");
        eprintln!("  {drain_cmd}");
        eprintln!("note: {retention_note}");
    }
}

// --- publish config resolution --------------------------------------------

/// Resolve the publish config (per-key merge of repo-local `.glasspad.yaml` → home
/// config → built-in default), mapping a config error to the shared `--json` error
/// contract. An unreadable file is a system error (exit 2); a malformed/invalid one
/// is a user error (exit 1).
pub(super) fn resolve_publish_config(json: bool) -> config::ResolvedConfig {
    // A failure here (a deleted/inaccessible working directory) is a system error,
    // not a silent fall-back to a CWD-relative "." that would resolve config from an
    // unintended place.
    let cwd = std::env::current_dir().unwrap_or_else(|e| {
        exit_error(
            json,
            2,
            "cwd_unavailable",
            &format!("cannot determine the current directory (needed to find .glasspad.yaml): {e}"),
            None,
            None,
        );
    });
    match config::resolve(&cwd, &publish_config_candidates()) {
        Ok(c) => c,
        Err(e) => {
            let exit = if e.code == "unreadable_config" { 2 } else { 1 };
            exit_error(json, exit, e.code, &e.message, None, None);
        }
    }
}

/// Resolve + validate the configured favicon emoji from the publish config
/// (`.glasspad.yaml` → home config). Returns the validated emoji, or `None` when
/// unset (the built-in default is rendered downstream). An invalid emoji is a hard,
/// informative error (AI-first §1) — never silently dropped. Used by the loopback
/// serve/create/render paths (the favicon becomes the host default) and by `build`.
pub(super) fn resolve_favicon(json: bool) -> Option<String> {
    let cfg = resolve_publish_config(json);
    validate_favicon(cfg.favicon.as_deref(), json)
}

/// Like [`resolve_favicon`], but **non-fatal** — for the long-running loopback
/// serve/create/render paths, where a decorative favicon (or a malformed repo
/// `.glasspad.yaml`) must not stop the server from binding. On any error it warns to
/// stderr and falls back to the built-in default (`None`), mirroring how the loopback
/// submission store degrades to a warning rather than aborting `serve`. `build` and
/// hosted `publish` are one-shot commands with clear failure output, so they keep the
/// fatal [`resolve_favicon`] / [`validate_favicon`].
pub(super) fn resolve_favicon_lenient() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let cfg = match config::resolve(&cwd, &publish_config_candidates()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "warning: ignoring .glasspad.yaml for the favicon ({}): {}",
                e.code, e.message
            );
            return None;
        }
    };
    match cfg.favicon.as_deref() {
        None => None,
        Some(v) => match favicon::validate(v) {
            Ok(ok) => Some(ok),
            Err(msg) => {
                eprintln!("warning: ignoring invalid favicon (using the default): {msg}");
                None
            }
        },
    }
}

/// Validate an optional configured favicon, exiting with an informative error on a
/// non-emoji / injection value. Shared by the fatal paths (`build`, and the hosted
/// per-space publish) so one rule applies uniformly.
pub(super) fn validate_favicon(raw: Option<&str>, json: bool) -> Option<String> {
    match raw {
        None => None,
        Some(v) => match favicon::validate(v) {
            Ok(ok) => Some(ok),
            Err(msg) => exit_error(json, 1, "invalid_favicon", &msg, Some(v), None),
        },
    }
}

/// Resolve the publish target: `--target` flag > `$GLASSPAD_TARGET` > config
/// `target:` > the built-in `loopback` default (so zero-config local just works).
pub(super) fn resolve_target(
    flag: Option<String>,
    cfg: &config::ResolvedConfig,
    json: bool,
) -> Target {
    if let Some(raw) = resolve_setting(flag, "GLASSPAD_TARGET", None) {
        return Target::parse(&raw).unwrap_or_else(|m| {
            exit_error(json, 1, "invalid_target", &m, Some(&raw), None);
        });
    }
    cfg.target.unwrap_or(Target::Loopback)
}

/// Resolve the hosted server URL: `--server` > `$GLASSPAD_SERVER` > config
/// `server:`. Absent at every level is an informative error (hosted needs it).
pub(super) fn resolve_server(
    flag: Option<String>,
    cfg: &config::ResolvedConfig,
    json: bool,
) -> String {
    resolve_setting(flag, "GLASSPAD_SERVER", cfg.server.clone()).unwrap_or_else(|| {
        exit_error(
            json,
            1,
            "missing_server",
            "no hosted server URL: pass --server <url>, set $GLASSPAD_SERVER, or add `server:` \
             to .glasspad.yaml / ~/.config/glasspad/config.yaml",
            None,
            None,
        );
    })
}

/// Resolve the hosted API key: `--api-key` > `$GLASSPAD_API_KEY` > the config
/// `api_key` source (an inline secret, an `{env: …}` indirection, or an `{file: …}`
/// / `api_key_file` key file). An unset/empty value at every level is an informative
/// error; an unreadable key file is a system error (exit 2). The key is never printed.
pub(super) fn resolve_api_key(
    flag: Option<String>,
    cfg: &config::ResolvedConfig,
    json: bool,
) -> String {
    if let Some(k) = resolve_setting(flag, "GLASSPAD_API_KEY", None) {
        return k;
    }
    match &cfg.api_key {
        Some(ApiKeySource::Inline(s)) => {
            let t = s.trim();
            if t.is_empty() {
                missing_api_key(json);
            }
            t.to_string()
        }
        Some(ApiKeySource::Env(name)) => match std::env::var(name) {
            Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => exit_error(
                json,
                1,
                "missing_api_key",
                &format!(
                    "config `api_key` points at ${name}, which is unset or empty; set it, or pass \
                     --api-key / $GLASSPAD_API_KEY"
                ),
                None,
                None,
            ),
        },
        Some(ApiKeySource::File(p)) => read_api_key_file(p, json),
        None => missing_api_key(json),
    }
}

/// Upper bound on an `api_key` file — a bearer token is short; a larger file is a
/// misconfiguration, not a key.
pub(super) const MAX_API_KEY_FILE_BYTES: u64 = 64 * 1024;

/// Read the trimmed contents of a config `api_key` file, bounded and fail-closed.
/// Since the path can come from repo-local config, this refuses a non-regular file
/// (a FIFO/device would block a naive read forever) and caps the read so a huge file
/// cannot exhaust memory. An empty file is a user error; an I/O failure is a system
/// error. The key value is never included in any message.
pub(super) fn read_api_key_file(p: &Path, json: bool) -> String {
    let meta = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(e) => exit_error(
            json,
            2,
            "api_key_file_unreadable",
            &format!("cannot read config `api_key` file {}: {e}", p.display()),
            None,
            None,
        ),
    };
    if !meta.is_file() {
        exit_error(
            json,
            1,
            "api_key_file_unreadable",
            &format!(
                "config `api_key` file {} is not a regular file (FIFOs, sockets, and devices are not read as keys)",
                p.display()
            ),
            None,
            None,
        );
    }
    if meta.len() > MAX_API_KEY_FILE_BYTES {
        exit_error(
            json,
            1,
            "api_key_file_unreadable",
            &format!(
                "config `api_key` file {} is {} bytes, over the {}-byte cap (that is not a key)",
                p.display(),
                meta.len(),
                MAX_API_KEY_FILE_BYTES
            ),
            None,
            None,
        );
    }
    let bytes = match read_capped(p, MAX_API_KEY_FILE_BYTES) {
        Ok(b) => b,
        Err(e) => exit_error(
            json,
            2,
            "api_key_file_unreadable",
            &format!("cannot read config `api_key` file {}: {e}", p.display()),
            None,
            None,
        ),
    };
    let text = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => exit_error(
            json,
            1,
            "api_key_file_unreadable",
            &format!("config `api_key` file {} is not valid UTF-8", p.display()),
            None,
            None,
        ),
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        exit_error(
            json,
            1,
            "missing_api_key",
            &format!("config `api_key` file {} is empty", p.display()),
            None,
            None,
        );
    }
    trimmed.to_string()
}

/// The shared "no API key anywhere" error (exit 1).
pub(super) fn missing_api_key(json: bool) -> ! {
    exit_error(
        json,
        1,
        "missing_api_key",
        "no API key: pass --api-key <key>, set $GLASSPAD_API_KEY, or add `api_key:` (inline, or \
         `{env: VAR}` / `{file: PATH}`) to .glasspad.yaml / ~/.config/glasspad/config.yaml",
        None,
        None,
    );
}

/// `glasspad push-round <slug> <file> [--server <url>] [--api-key <key>] [--markdown
/// [--template <ref>]]` — the B2 **multi-round** client. Re-render an already-published
/// hosted page in response to a submission: it POSTs the new body to
/// `/api/v1/pages/<slug>/rounds` (API-key auth, owner-scoped) and the server swaps the
/// live page's content in place for every connected viewer, then prints
/// `{slug, round, content_version}`. Config precedence mirrors `publish`
/// (flag > `$GLASSPAD_SERVER`/`$GLASSPAD_API_KEY` > config file). The new
/// `content_version` is the value the next submission for this round will echo.
pub async fn push_round(
    slug: String,
    file: PathBuf,
    server: Option<String>,
    api_key: Option<String>,
    markdown: bool,
    template: Option<String>,
    json: bool,
) {
    let cfg = resolve_publish_config(json);
    let server = resolve_server(server, &cfg, json);
    let api_key = resolve_api_key(api_key, &cfg, json);

    // Read the new round source (bounded, UTF-8) — the same strict checks `publish` uses.
    let noun = if markdown { "markdown" } else { "html" };
    let content = read_capped_utf8_file(&file, noun, "no_such_path", json);

    let mut body = serde_json::Map::new();
    if markdown {
        body.insert("markdown".into(), json!(content));
        if let Some(t) = &template {
            let resolved = resolve_publish_template(t, json);
            body.insert("template".into(), json!(resolved));
        }
    } else {
        if template.is_some() {
            exit_error(
                json,
                1,
                "template_without_markdown",
                "--template only applies with --markdown (raw HTML is pushed verbatim)",
                None,
                None,
            );
        }
        body.insert("html".into(), json!(content));
    }

    if server.starts_with("http://") && !server_is_loopback(&server) {
        eprintln!(
            "warning: pushing a round over plaintext http:// to a non-local host sends the API \
             key in the clear; prefer https://"
        );
    }

    let url = format!(
        "{}/api/v1/pages/{}/rounds",
        server.trim_end_matches('/'),
        slug
    );
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => exit_error(json, 2, "client_init_failed", &e.to_string(), None, None),
    };
    let resp = client
        .post(&url)
        .bearer_auth(&api_key)
        .json(&serde_json::Value::Object(body))
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => exit_error(
            json,
            2,
            "request_failed",
            &format!("cannot reach {url}: {e}"),
            None,
            None,
        ),
    };

    let status = resp.status();
    let payload: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let msg = payload
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("the server rejected the round push");
        let code = payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("push_round_rejected");
        let exit = if status.is_client_error() { 1 } else { 2 };
        exit_error(
            json,
            exit,
            code,
            &format!("{msg} (HTTP {})", status.as_u16()),
            None,
            None,
        );
    }

    let round = payload.get("round").and_then(|r| r.as_u64());
    let content_version = payload
        .get("content_version")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
        .map(str::to_string);
    let (round, content_version) = match (round, content_version) {
        (Some(r), Some(cv)) => (r, cv),
        _ => exit_error(
            json,
            2,
            "malformed_response",
            &format!(
                "server returned {} but no round/content_version in the body",
                status.as_u16()
            ),
            None,
            None,
        ),
    };

    if json {
        let out = json!({
            "schema_version": SCHEMA_VERSION,
            "pushed": true,
            "slug": slug,
            "round": round,
            "content_version": content_version,
            "warnings": [],
        });
        emit_json_line(&out);
    } else {
        eprintln!("pushed round {round} of '{slug}' (content_version {content_version})");
    }
}

/// Resolve one setting by precedence: explicit flag > environment variable > config
/// file value. An empty/whitespace flag or env value is treated as unset (AI-first
/// §1 — no silent empties). Returns `None` if unset at every level.
pub(super) fn resolve_setting(
    flag: Option<String>,
    env: &str,
    file: Option<String>,
) -> Option<String> {
    let nonempty = |s: String| {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    };
    flag.and_then(nonempty)
        .or_else(|| std::env::var(env).ok().and_then(nonempty))
        .or_else(|| file.and_then(nonempty))
}

/// Best-effort: is the `--server` URL a loopback host (where plaintext http is
/// acceptable)? Used only to decide whether to warn about a cleartext bearer.
pub(super) fn server_is_loopback(server: &str) -> bool {
    let authority = server
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(server);
    let host = authority
        .split(['/', ':'])
        .next()
        .unwrap_or(authority)
        .to_ascii_lowercase();
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]"
}

/// Candidate config-file paths in precedence order: the documented XDG path
/// (`$XDG_CONFIG_HOME`, else `~/.config`) first on every platform, then — for
/// backward compatibility — the platform `dirs::config_dir()` location (on macOS
/// `~/Library/Application Support`), which older installs may still use. The
/// first candidate that exists wins.
pub(super) fn publish_config_candidates() -> Vec<PathBuf> {
    publish_config_candidates_from(
        // Per the XDG spec an empty value is treated as unset (falls back to ~/.config).
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        dirs::home_dir(),
        dirs::config_dir(),
    )
}

/// Pure candidate-ordering logic (env/home/config-dir passed in so it is testable).
pub(super) fn publish_config_candidates_from(
    xdg_config_home: Option<PathBuf>,
    home_dir: Option<PathBuf>,
    platform_config_dir: Option<PathBuf>,
) -> Vec<PathBuf> {
    let leaf = |base: PathBuf| base.join("glasspad").join("config.yaml");
    let mut candidates = Vec::new();

    // Documented, cross-platform path: $XDG_CONFIG_HOME (if set & absolute), else ~/.config.
    let xdg = xdg_config_home
        .filter(|p| p.is_absolute())
        .or_else(|| home_dir.map(|h| h.join(".config")));
    if let Some(dir) = xdg {
        candidates.push(leaf(dir));
    }

    // Backward-compat fallback: the platform config dir (macOS Application Support).
    // Filter for absoluteness too — on Unix `dirs::config_dir()` echoes a relative
    // `$XDG_CONFIG_HOME` verbatim, and a relative candidate would be read against the
    // process CWD (an unintended file on multi-user/container hosts).
    if let Some(dir) = platform_config_dir.filter(|p| p.is_absolute()) {
        let legacy = leaf(dir);
        if !candidates.contains(&legacy) {
            candidates.push(legacy);
        }
    }

    candidates
}

/// Resolve the `--template` reference for `publish`: a built-in name is sent
/// verbatim; anything else is a path to a template file, read + returned as an
/// inline template string. Mirrors `resolve_template`'s built-in-vs-path rule.
pub(super) fn resolve_publish_template(reference: &str, json: bool) -> String {
    if render::builtin_template(reference).is_some() {
        return reference.to_string();
    }
    // A path to a template file (read strictly, bounded, UTF-8).
    read_capped_utf8_file(Path::new(reference), "template", "template_not_found", json)
}
