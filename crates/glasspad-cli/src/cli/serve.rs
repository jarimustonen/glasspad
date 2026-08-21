use super::author::{create, launch_browser, render};
use super::publish::{
    PathKind, classify_publish_path, publish_config_candidates, resolve_favicon_lenient,
    resolve_setting,
};
use super::runtime::*;
use super::*;

// --- serve ----------------------------------------------------------------

/// Build the loopback [`ArtifactHost`], attaching the return-channel submission
/// store when it can be opened. A store that fails to open (permissions, disk) is
/// a **warning**, not fatal: serving pages must not depend on the return channel,
/// so the host comes up with no submission store (submit endpoints then answer
/// `503 return_channel_unavailable`).
pub(super) fn loopback_host(
    port: u16,
    favicon: Option<String>,
    lan_origin: Option<String>,
) -> Arc<ArtifactHost> {
    let mut host = ArtifactHost::new(port)
        .with_favicon(favicon)
        .with_lan_origin(lan_origin);
    if let Some(store) = loopback_submissions(port) {
        host = host.with_submissions(store);
    }
    Arc::new(host)
}

/// Open the per-port loopback submission store under the state dir
/// (`$GLASSPAD_STATE_DIR`, else `~/.glasspad`) `submissions/<port>/`. Per-port so
/// concurrent `serve`s on different ports never share a channel; the matching
/// `await-submission` reaches it over loopback HTTP, so it needs no path itself.
pub(super) fn loopback_submissions(port: u16) -> Option<Arc<SubmissionStore>> {
    let base = std::env::var_os("GLASSPAD_STATE_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".glasspad")))?;
    let dir = base.join("submissions").join(port.to_string());
    match SubmissionStore::open(&dir) {
        Ok(store) => Some(store),
        Err(e) => {
            eprintln!(
                "warning: return channel disabled — cannot open {}: {e}",
                dir.display()
            );
            None
        }
    }
}

/// `glasspad loopback serve [path]` — the advanced, explicit loopback entry point.
/// Serve a directory, a single file, or (with no path) the built-in fixtures live
/// on 127.0.0.1, blocking until killed. A single `.md`/`.markdown` file is rendered
/// through the template; a single `.html`/`.htm` file is served verbatim; a
/// directory is scanned as a space — the same classification the default `publish`
/// verb uses, so `loopback serve` is `publish`'s loopback path without the config
/// target resolution. `open` launches the browser after binding.
pub async fn loopback_serve(
    path: Option<PathBuf>,
    template: Option<String>,
    name: Option<String>,
    port: u16,
    bind: Option<String>,
    open: bool,
    json: bool,
) {
    // Resolve the optional LAN exposure ONCE (flag > $GLASSPAD_BIND > config `bind:`)
    // before dispatch, so serve/create/render share one validated value. Off by
    // default → loopback-only, byte-compatible with the pre-LAN path.
    let lan = resolve_lan_exposure(bind, port, json);
    match path {
        None => serve(None, port, lan, open, json).await,
        Some(p) => match classify_publish_path(&p, json) {
            PathKind::Dir => serve(Some(p), port, lan, open, json).await,
            PathKind::Markdown => render(p, template, name, port, lan, open, json).await,
            PathKind::Html => {
                if template.is_some() {
                    exit_error(
                        json,
                        1,
                        "template_without_markdown",
                        "--template only applies to a markdown file (raw HTML is served verbatim)",
                        None,
                        None,
                    );
                }
                create(p, name, port, lan, open, json).await;
            }
        },
    }
}

/// Resolve the optional LAN exposure for `loopback serve --bind`: `--bind` flag >
/// `$GLASSPAD_BIND` > config `bind:` key. `None` at every level → loopback-only (the
/// byte-compatible default). A present value is validated eagerly
/// ([`server::resolve_lan_exposure`]) — a wildcard/loopback/malformed value is a
/// hard, informative error (AI-first §1), never a silent public bind.
pub(super) fn resolve_lan_exposure(
    flag: Option<String>,
    port: u16,
    json: bool,
) -> Option<server::LanExposure> {
    // The config `bind:` value is HOME-ONLY (a repo-local `.glasspad.yaml` cannot opt a
    // user into a LAN bind — enforced in `config::merge`). It is read leniently (a
    // decorative-config read must not, by itself, be fatal); the flag/env take
    // precedence and the resolved value is validated strictly below.
    let from_config = || {
        let cfg = std::env::current_dir()
            .ok()
            .and_then(|cwd| config::resolve(&cwd, &publish_config_candidates()).ok());
        // Loudly note a repo-local `bind:` that we deliberately ignored — the operator
        // should know the repo tried to opt them into a network bind.
        if let Some(c) = &cfg
            && c.bind_repo_ignored
        {
            eprintln!(
                "warning: ignoring a repo-local .glasspad.yaml `bind:` — LAN serve must be \
                 opted into per machine (pass --bind, set $GLASSPAD_BIND, or add `bind:` to \
                 your HOME config), not activated by a project file"
            );
        }
        cfg.and_then(|c| c.bind)
    };
    let raw = resolve_setting(flag, "GLASSPAD_BIND", from_config())?;
    match server::resolve_lan_exposure(&raw, port) {
        Ok(lan) => Some(lan),
        Err(msg) => exit_error(json, 1, "invalid_bind", &msg, Some(&raw), None),
    }
}

/// The loud, security-relevant startup banner for LAN mode, naming the exact
/// reachable URL. Printed to stderr on serve/create/render **before** the normal
/// startup envelope so it is impossible to miss; the JSON envelopes carry the same
/// facts structurally (the `lan` URL + `lan_host`).
pub(super) fn warn_lan_exposure(lan: &server::LanExposure, space_url_path: &str) {
    let url = format!("{}{}", lan.origin, space_url_path);
    eprintln!(
        "⚠️  LAN MODE: this glasspad server is now reachable from OTHER DEVICES on your \
         local network at {url}"
    );
    eprintln!(
        "    (bind {}). It carries NO API key and is a trusted-LAN convenience — only run it \
         on a network you TRUST. Traffic is plaintext HTTP: anyone able to reach or MITM this \
         LAN (rogue AP, ARP/mDNS spoofing) can read submissions and page content and inject \
         same-origin HTML. Never expose it beyond a trusted LAN. Loopback (127.0.0.1) still \
         works for local tooling.",
        lan.display
    );
}

/// The LAN URL (`lan` field in the JSON envelope), given the space's URL path
/// (e.g. `/myspace/` or `/`).
pub(super) fn lan_url(lan: &server::LanExposure, space_url_path: &str) -> String {
    format!("{}{}", lan.origin, space_url_path)
}

/// Bind loopback + any LAN address, exiting with the `bind_failed` envelope naming
/// the exact address on failure. Shared by serve/create/render.
pub(super) async fn bind_all_or_exit(
    port: u16,
    lan: Option<&server::LanExposure>,
    json: bool,
) -> Vec<tokio::net::TcpListener> {
    match server::bind_all(port, lan).await {
        Ok(listeners) => listeners,
        Err(e) => exit_error(
            json,
            2,
            "bind_failed",
            &format!("cannot bind {}: {}", e.addr, e.source),
            Some(&e.addr),
            None,
        ),
    }
}

/// Serve a live directory as a space, or (with no dir) the built-in fixtures.
/// Binds loopback, then blocks serving until killed. `open` launches the browser
/// on the served URL after binding (the loopback `publish` path sets it; explicit
/// `loopback serve` defaults it off). Reached via `glasspad loopback serve` and the
/// loopback `publish` dispatch.
pub async fn serve(
    dir: Option<PathBuf>,
    port: u16,
    lan: Option<server::LanExposure>,
    open: bool,
    json: bool,
) {
    let host = loopback_host(
        port,
        resolve_favicon_lenient(),
        lan.as_ref().map(|l| l.origin.clone()),
    );

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
    // startup envelope is only printed once the port(s) are actually held. Loopback
    // is always bound; LAN mode additionally binds the opted-in address.
    let listeners = bind_all_or_exit(port, lan.as_ref(), json).await;

    // Record this process (post-bind, so a bind failure leaves no pid file) and
    // arrange clean SIGTERM/SIGINT shutdown; a write/permission failure is fatal here.
    let pid_warnings = acquire_pidfile(json).await;

    if let Some(d) = dir {
        server::spawn_watcher(host.clone(), d);
    }
    let url_path = match live.as_ref() {
        Some((name, _, _)) => format!("/{name}/"),
        None => "/".to_string(),
    };
    // Loud LAN warning FIRST, so it precedes the ordinary "serving" line/envelope.
    if let Some(l) = lan.as_ref() {
        warn_lan_exposure(l, &url_path);
    }
    emit_serving(json, port, live.as_ref(), lan.as_ref(), pid_warnings);
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
        // A mid-run failure exits without hitting the signal handler; drop our pid
        // file so it does not linger stale.
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

/// Print the `serve` startup envelope. `--json` → one line to stdout (the data
/// channel); text → a line to stderr (a long-running process's stdout stays free
/// for a caller that pipes it). The command is long-running, so this is a startup
/// announcement, not a terminal result — the server then runs until killed. `pid`
/// is included so a caller knows exactly what `glasspad stop` will target; extra
/// (pid-file takeover) warnings are folded into the envelope's `warnings`.
pub(super) fn emit_serving(
    json: bool,
    port: u16,
    live: Option<&(String, Vec<String>, Option<String>)>,
    lan: Option<&server::LanExposure>,
    mut warnings: Vec<String>,
) {
    let pid = std::process::id();
    // LAN envelope fields (JSON): `lan` = the reachable URL, `lan_host` = the
    // allowlisted host, or `null` when loopback-only.
    let (lan_field, lan_host_field) = match (lan, live) {
        (Some(l), Some((name, _, _))) => {
            (json!(lan_url(l, &format!("/{name}/"))), json!(l.allow_host))
        }
        (Some(l), None) => (json!(lan_url(l, "/")), json!(l.allow_host)),
        (None, _) => (serde_json::Value::Null, serde_json::Value::Null),
    };
    match live {
        Some((name, slugs, home)) => {
            let url = format!("http://127.0.0.1:{port}/{name}/");
            if json {
                let payload = json!({
                    "schema_version": SCHEMA_VERSION,
                    "serving": true,
                    "port": port,
                    "pid": pid,
                    "space": name,
                    "url": url,
                    "lan": lan_field,
                    "lan_host": lan_host_field,
                    "artifacts": slugs,
                    "home": home,
                    "warnings": warnings,
                });
                emit_json_line(&payload);
            } else {
                for w in &warnings {
                    eprintln!("warning: {w}");
                }
                eprintln!(
                    "glasspad serving space '{name}' at {url} ({} artifact{}, pid {pid})",
                    slugs.len(),
                    if slugs.len() == 1 { "" } else { "s" }
                );
            }
        }
        None => {
            let url = format!("http://127.0.0.1:{port}/");
            // The fixtures caveat leads; any pid-file warning follows it.
            warnings.insert(
                0,
                "no directory given: serving built-in fixtures only; \
                 pass a directory to serve a space"
                    .to_string(),
            );
            if json {
                let payload = json!({
                    "schema_version": SCHEMA_VERSION,
                    "serving": true,
                    "port": port,
                    "pid": pid,
                    "space": serde_json::Value::Null,
                    "url": url,
                    "lan": lan_field,
                    "lan_host": lan_host_field,
                    "artifacts": [],
                    "home": serde_json::Value::Null,
                    "warnings": warnings,
                });
                emit_json_line(&payload);
            } else {
                for w in &warnings {
                    eprintln!("warning: {w}");
                }
                eprintln!("glasspad serving built-in fixtures at {url} (pid {pid})");
            }
        }
    }
}
