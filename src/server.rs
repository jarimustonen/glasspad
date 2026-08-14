use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::rejection::JsonRejection;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;

use crate::artifact_host::space::{self, Artifact, ScanError, Snapshot, Space};
use crate::artifact_host::{self, ArtifactHost, guards, render, valid_space};
use crate::cli::SCHEMA_VERSION;
use crate::hosted::submit::origin_ok;
use crate::submissions::{
    self, DEFAULT_WAIT_SECS, MAX_LIST, MAX_SUBMISSION_BYTES, MAX_WAIT_SECS, SubmitError,
    WaitOutcome,
};

/// How often the (dependency-free) filesystem watcher polls the served directory
/// for changes. 500 ms is imperceptible for a local edit-reload loop and avoids
/// pulling in a native file-notification dependency for a localhost dev tool.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);

/// Build the complete, fully-guarded application router over a shared artifact
/// host. Extracted so tests can exercise the middleware stack (the global Host
/// guard) — `artifact_host::router` alone omits it, which would let the security
/// gate pass with the guard absent or misordered.
///
/// The v0.1 control API (`/api/pads`) and legacy `/{id}` pad renderer were
/// removed in Wave 3 (design.md §10, decision D2): the only same-origin surface
/// now is the sandboxed artifact host, so the sole coexistence risk it posed is
/// closed.
pub fn build_app_with_host(policy: guards::HostPolicy, host: Arc<ArtifactHost>) -> Router {
    // --- v0.2 HTML-artifact host (Wave 1 security gate + Wave 2a space model) ---
    artifact_host::router(host.clone())
        // The loopback return channel (POST submit + poll/wait), under the same
        // `_gp`/space topology and behind the same Host guard below.
        .merge(loopback_submissions_router(host))
        // Global DNS-rebinding defense: validate the Host header on every route
        // against the allowlist ([`guards::HostPolicy`] — loopback, plus the one
        // opted-in `--bind` host in LAN mode).
        .layer(middleware::from_fn_with_state(policy, guards::host_guard))
}

// --- loopback return channel ----------------------------------------------
//
// The loopback analogue of `hosted::submit`. The trusted shell POSTs an artifact's
// submission here; the agent that ran `glasspad serve` reads it back (directly or
// via `glasspad await-submission`). No API key — the server is loopback-only (the
// Host guard already refuses any non-loopback Host) — but the same anti-spoof + CSRF
// + flood defenses as the hosted path apply: the **space** is bound from the URL
// path, the **artifact slug** from the shell's trusted `slug` field (the artifact
// payload never sets either), the `Origin` must be a loopback origin, and the
// payload is size + rate capped. The stamped tenant is the loopback sentinel.

/// The loopback sentinel owner for a submission (there are no tenants on loopback).
const LOOPBACK_TENANT: &str = "local";

/// Build the loopback submit + poll/wait routes over the shared `ArtifactHost`
/// (which carries the optional submission store).
fn loopback_submissions_router(host: Arc<ArtifactHost>) -> Router {
    let submit_body_limit = MAX_SUBMISSION_BYTES + 16 * 1024;
    Router::new()
        .route(
            "/{space}/_gp/submit",
            post(loopback_submit).layer(DefaultBodyLimit::max(submit_body_limit)),
        )
        .route("/{space}/_gp/submissions", get(loopback_list))
        .route("/{space}/_gp/submissions/wait", get(loopback_wait))
        .route("/{space}/_gp/submissions/stream", get(loopback_stream))
        .with_state(host)
}

/// Loopback submit body from the trusted shell. `data` is the untrusted payload;
/// `slug` is the artifact within the space the shell currently frames (its trusted
/// `current`, never the artifact's own claim); `content_version` is the artifact's
/// version echo, checked against the server's authoritative value.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoopbackSubmit {
    data: serde_json::Value,
    #[serde(default)]
    content_version: Option<String>,
    #[serde(default)]
    slug: Option<String>,
}

/// `POST /{space}/_gp/submit`.
async fn loopback_submit(
    State(host): State<Arc<ArtifactHost>>,
    AxumPath(space): AxumPath<String>,
    headers: HeaderMap,
    body: Result<Json<LoopbackSubmit>, JsonRejection>,
) -> Response {
    let Some(store) = host.submissions().cloned() else {
        return sub_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "return_channel_unavailable",
            "the return channel is not available on this server",
        );
    };
    // CSRF: a cross-site browser fetch carries its own Origin; require a loopback one.
    if !origin_ok(&headers, &host.origin_list()) {
        return sub_err(
            StatusCode::FORBIDDEN,
            "bad_origin",
            "cross-origin submit rejected",
        );
    }
    if !valid_space(&space) {
        return sub_err(StatusCode::NOT_FOUND, "not_found", "no such space");
    }
    let Json(req) = match body {
        Ok(b) => b,
        Err(rej) => {
            return sub_err(
                StatusCode::BAD_REQUEST,
                "invalid_json",
                &format!("request body is not valid JSON: {rej}"),
            );
        }
    };

    // Resolve the artifact (the shell's current slug, else the space home) and its
    // authoritative content-version from the served body — via the same seam the
    // content route uses (live snapshot, else fixtures), never the payload.
    let snap = host.snapshot();
    let slug = req
        .slug
        .clone()
        .or_else(|| artifact_host::resolve_home(&snap, &space))
        .unwrap_or_else(|| SINGLE_SLUG.to_string());
    let Some(body) = artifact_host::resolve_artifact_html(&snap, &space, &slug) else {
        return sub_err(
            StatusCode::NOT_FOUND,
            "no_such_artifact",
            "no such artifact in this space",
        );
    };
    let version = submissions::content_version(&body);
    if let Some(echo) = req.content_version.as_deref()
        && echo != version
    {
        return sub_err(
            StatusCode::CONFLICT,
            "content_version_mismatch",
            "the submission answers a stale version of this artifact",
        );
    }

    let data = req.data;
    let result = tokio::task::spawn_blocking(move || {
        store.submit(&space, &slug, LOOPBACK_TENANT, &version, data)
    })
    .await;
    match result {
        Ok(Ok(sub)) => (
            StatusCode::CREATED,
            Json(json!({
                "schema_version": SCHEMA_VERSION,
                "id": sub.id,
                "content_version": sub.content_version,
            })),
        )
            .into_response(),
        Ok(Err(e)) => loopback_submit_error(e),
        Err(join) => {
            eprintln!("glasspad: loopback submit task panicked: {join}");
            sub_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not persist the submission",
            )
        }
    }
}

#[derive(Deserialize)]
struct LoopbackListQuery {
    since: Option<u64>,
}

/// `GET /{space}/_gp/submissions?since=<cursor>` — plain poll (loopback).
async fn loopback_list(
    State(host): State<Arc<ArtifactHost>>,
    AxumPath(space): AxumPath<String>,
    Query(q): Query<LoopbackListQuery>,
) -> Response {
    let Some(store) = host.submissions().cloned() else {
        return sub_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "return_channel_unavailable",
            "the return channel is not available on this server",
        );
    };
    if !valid_space(&space) {
        return sub_err(StatusCode::NOT_FOUND, "not_found", "no such space");
    }
    let since = q.since.unwrap_or(0);
    match tokio::task::spawn_blocking(move || store.list_since(&space, since, MAX_LIST)).await {
        Ok(Ok(page)) => sub_list_response(&page, false),
        _ => sub_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            "could not read submissions",
        ),
    }
}

#[derive(Deserialize)]
struct LoopbackWaitQuery {
    since: Option<u64>,
    timeout: Option<u64>,
}

/// `GET /{space}/_gp/submissions/wait?since=<cursor>&timeout=<secs>` — server-side
/// long-poll (loopback).
async fn loopback_wait(
    State(host): State<Arc<ArtifactHost>>,
    AxumPath(space): AxumPath<String>,
    Query(q): Query<LoopbackWaitQuery>,
) -> Response {
    let Some(store) = host.submissions().cloned() else {
        return sub_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "return_channel_unavailable",
            "the return channel is not available on this server",
        );
    };
    if !valid_space(&space) {
        return sub_err(StatusCode::NOT_FOUND, "not_found", "no such space");
    }
    let since = q.since.unwrap_or(0);
    let secs = q
        .timeout
        .unwrap_or(DEFAULT_WAIT_SECS)
        .clamp(1, MAX_WAIT_SECS);
    match submissions::wait(store, space, since, Duration::from_secs(secs), MAX_LIST).await {
        Ok(WaitOutcome::Ready(page)) => sub_list_response(&page, false),
        Ok(WaitOutcome::TimedOut { cursor }) => sub_list_response(
            &submissions::ListPage {
                submissions: Vec::new(),
                cursor,
            },
            true,
        ),
        Ok(WaitOutcome::TooBusy) => sub_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "too_many_waiters",
            "too many long-polls are held; retry with the plain poll",
        ),
        Err(_) => sub_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            "could not read submissions",
        ),
    }
}

/// `GET /{space}/_gp/submissions/stream?since=<cursor>` — server-push SSE (loopback).
/// The loopback analogue of the hosted stream: no API key (loopback-only, the Host
/// guard already refuses any non-loopback Host), space bound from the URL path. Pushes
/// each submission for the space after the cursor as a `submission` event, sharing the
/// same held-connection cap as the long-poll; at the cap it answers 503 and the caller
/// falls back to polling. Cursor is `since`, falling back to `Last-Event-ID`.
async fn loopback_stream(
    State(host): State<Arc<ArtifactHost>>,
    AxumPath(space): AxumPath<String>,
    Query(q): Query<LoopbackListQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(store) = host.submissions().cloned() else {
        return sub_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "return_channel_unavailable",
            "the return channel is not available on this server",
        );
    };
    if !valid_space(&space) {
        return sub_err(StatusCode::NOT_FOUND, "not_found", "no such space");
    }
    let since = q.since.or_else(|| last_event_id(&headers)).unwrap_or(0);
    match submissions::open_stream(store, space, since) {
        Some(rx) => submissions::submission_sse(rx).into_response(),
        None => sub_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "too_many_waiters",
            "too many streams are held; retry with the plain poll",
        ),
    }
}

/// Read a `Last-Event-ID` header cursor via the shared parser (a reconnecting
/// `EventSource` sends the last delivered submission id). A missing/unparseable value
/// yields `None`, so the caller starts from the default 0 — a harmless full re-read,
/// never a cross-space escape (the space is still bound from the URL path).
fn last_event_id(headers: &HeaderMap) -> Option<u64> {
    submissions::parse_last_event_id(headers.get("last-event-id").and_then(|v| v.to_str().ok()))
}

fn sub_list_response(page: &submissions::ListPage, timed_out: bool) -> Response {
    let items: Vec<serde_json::Value> = page
        .submissions
        .iter()
        .map(|s| s.to_public_json())
        .collect();
    Json(json!({
        "schema_version": SCHEMA_VERSION,
        "submissions": items,
        "cursor": page.cursor,
        "timed_out": timed_out,
    }))
    .into_response()
}

fn loopback_submit_error(e: SubmitError) -> Response {
    match e {
        SubmitError::TooLarge => sub_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "submission_too_large",
            &e.to_string(),
        ),
        SubmitError::Full => sub_err(
            StatusCode::INSUFFICIENT_STORAGE,
            "submissions_full",
            &e.to_string(),
        ),
        SubmitError::RateLimited => sub_err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            &e.to_string(),
        ),
        SubmitError::Io(io) => {
            eprintln!("glasspad: loopback submit storage error: {io}");
            sub_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not persist the submission",
            )
        }
    }
}

fn sub_err(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "schema_version": SCHEMA_VERSION,
            "error": { "code": code, "message": message },
        })),
    )
        .into_response()
}

/// Convenience for tests that don't serve a live directory (fixtures only).
#[cfg(test)]
pub fn build_app(port: u16) -> Router {
    build_app_with_host(
        guards::HostPolicy::loopback(port),
        Arc::new(ArtifactHost::new(port)),
    )
}

/// The slug of the single artifact a `create`d space holds: its canonical home.
pub const SINGLE_SLUG: &str = "index";

/// Bind the loopback control plane. Loopback is bound **unconditionally** on every
/// run mode (so `await-submission` / `open` / `stop`, which all speak loopback HTTP,
/// keep working) — LAN reach is an *additional* bind, see [`bind_all`]. Returns the
/// bind error so the CLI can surface it as its error envelope (e.g. port already in
/// use) rather than panicking.
pub async fn bind_loopback(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port)).await
}

/// A resolved LAN exposure for `glasspad loopback serve --bind <HOST>`
/// (loopback-lan-serve). OFF by default; opting in binds ONE extra explicitly-named
/// non-loopback address in addition to loopback and extends the DNS-rebinding Host
/// allowlist + the artifact CSP / submit-CSRF origin set by exactly that one host.
/// It is a **trusted-LAN convenience carrying no API key** — never a public bind.
#[derive(Clone, Debug)]
pub struct LanExposure {
    /// The literal host a LAN browser sends in `Host:` (lowercased, no port) — the
    /// one entry added to the DNS-rebinding allowlist ([`guards::HostPolicy`]).
    pub allow_host: String,
    /// The origin (`http://<host>:<port>`) the artifact CSP names and the submit
    /// CSRF check accepts, so a LAN client's shell + `/_gp/v1/*` base libs load.
    pub origin: String,
    /// The resolved non-loopback socket address(es) to bind in addition to loopback.
    pub addrs: Vec<std::net::SocketAddr>,
    /// The exact `--bind` value the operator gave — for the loud warning + URL.
    pub display: String,
}

/// Resolve a `--bind <HOST>` value into a [`LanExposure`], or a human error the CLI
/// surfaces. Strict (AI-first §1), and deliberately narrow — the issue's "explicit
/// address preferred over a blanket 0.0.0.0":
///
/// * A **wildcard** (`0.0.0.0` / `::`) is refused — name the concrete LAN address.
/// * A **loopback** value is refused — it is already served; `--bind` is for a LAN
///   address other devices reach.
/// * A value carrying a scheme / path / port is refused (pass a bare host or IPv4).
/// * A **hostname** is resolved to its non-loopback socket address(es) to bind; the
///   literal host string is what the Host allowlist matches (what a browser sends).
pub fn resolve_lan_exposure(host: &str, port: u16) -> Result<LanExposure, String> {
    use std::net::{IpAddr, ToSocketAddrs};

    let raw = host.trim();
    if raw.is_empty() {
        return Err(
            "empty --bind value: name the LAN IP or hostname other devices reach this \
             machine at (e.g. 192.168.1.50)"
                .to_string(),
        );
    }
    // A bare host is wanted — reject a URL / path / embedded port so the value we add
    // to the allowlist is exactly what a browser will send in `Host:`.
    if raw.contains("://") || raw.contains('/') {
        return Err(format!(
            "invalid --bind {raw:?}: pass a bare host or IPv4 address (e.g. 192.168.1.50), \
             not a URL or path"
        ));
    }
    let allow_host = raw.to_ascii_lowercase();
    if allow_host.contains(':') {
        return Err(format!(
            "invalid --bind {raw:?}: do not include a port (glasspad uses --port); pass just \
             the host or IPv4 address"
        ));
    }

    // Reject wildcard / loopback IPs before resolving.
    if let Ok(ip) = allow_host.parse::<IpAddr>() {
        if ip.is_unspecified() {
            return Err(format!(
                "refusing to bind the wildcard address {raw:?}: name the concrete LAN IP other \
                 devices use (e.g. 192.168.1.50), not 0.0.0.0/::. This is a trusted-LAN \
                 convenience, never a public bind."
            ));
        }
        if ip.is_loopback() {
            return Err(format!(
                "{raw:?} is a loopback address, which is already served — --bind names a LAN \
                 address other devices can reach"
            ));
        }
    }

    // Resolve to the socket address(es) to bind. A literal IP yields itself; a
    // hostname yields its resolved addrs. Loopback addrs are dropped (loopback is
    // already bound; re-binding it would collide) — if nothing non-loopback remains
    // the value is not a LAN address.
    let addrs: Vec<std::net::SocketAddr> = (allow_host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve --bind {raw:?} to an address to bind: {e}"))?
        .filter(|a| !a.ip().is_loopback())
        .collect();
    if addrs.is_empty() {
        return Err(format!(
            "--bind {raw:?} resolves only to loopback: name a LAN address other devices can reach"
        ));
    }

    Ok(LanExposure {
        allow_host: allow_host.clone(),
        // Origin from the lowercased host — browsers normalize the Origin host to
        // lowercase, and the submit CSRF check compares it verbatim.
        origin: format!("http://{allow_host}:{port}"),
        addrs,
        display: raw.to_string(),
    })
}

/// A bind failure that names the exact address that could not be bound, so the CLI
/// can surface `cannot bind <addr>: <source>` in its structured error envelope.
pub struct BindError {
    pub addr: String,
    pub source: std::io::Error,
}

/// Bind loopback (always) plus, in LAN mode, each resolved `--bind` address. Binding
/// loopback first means a port collision is reported against `127.0.0.1:<port>`
/// exactly as before; a LAN-address collision is reported against that address.
/// Returns all listeners so the caller serves the one app on every bound socket.
pub async fn bind_all(port: u16, lan: Option<&LanExposure>) -> Result<Vec<TcpListener>, BindError> {
    let mut listeners = Vec::new();
    let lo = bind_loopback(port).await.map_err(|source| BindError {
        addr: format!("127.0.0.1:{port}"),
        source,
    })?;
    listeners.push(lo);
    if let Some(lan) = lan {
        for addr in &lan.addrs {
            let l = TcpListener::bind(addr).await.map_err(|source| BindError {
                addr: addr.to_string(),
                source,
            })?;
            listeners.push(l);
        }
    }
    Ok(listeners)
}

/// Serve the app on an already-bound listener until the process is killed. Split
/// from `bind_loopback` so the CLI can bind first (surfacing a bind failure as an
/// error) and print its startup envelope only once the port is actually held.
/// Returns the serve error instead of panicking, so the CLI can surface a
/// mid-run failure as its structured error envelope (AI-first §10).
pub async fn serve_on(listener: TcpListener, app: Router) -> std::io::Result<()> {
    axum::serve(listener, app).await
}

/// Serve one shared app on every bound listener concurrently (loopback + any LAN
/// address), returning when the first serve task ends — normally never, until the
/// process is signaled, at which point all tasks die with it. The [`Router`] is
/// cheap to clone (an `Arc` of the shared [`ArtifactHost`] underneath), so both
/// sockets front the identical guarded app; there is exactly one security contract.
pub async fn serve_on_all(listeners: Vec<TcpListener>, app: Router) -> std::io::Result<()> {
    // Single listener (the common loopback-only case): serve directly, no JoinSet.
    let mut listeners = listeners;
    if listeners.len() == 1 {
        return serve_on(listeners.remove(0), app).await;
    }
    let mut set = tokio::task::JoinSet::new();
    for listener in listeners {
        let app = app.clone();
        set.spawn(async move { axum::serve(listener, app).await });
    }
    match set.join_next().await {
        Some(Ok(res)) => res,
        Some(Err(join)) => Err(std::io::Error::other(format!("serve task failed: {join}"))),
        None => Ok(()),
    }
}

/// Scan `dir` into a one-space [`Snapshot`], also returning the derived space
/// name. The name is the directory's final component, validated against the space
/// grammar + reserved list. Fail-fast: a malformed / colliding / reserved space
/// is an error the caller reports informatively (AI-first CLI contract).
pub fn scan_named(dir: &Path) -> Result<(String, Snapshot), ScanError> {
    let name = space::space_name_for(dir)?;
    let space = space::scan_dir(dir)?;
    let mut snap = Snapshot::empty();
    snap.spaces.insert(name.clone(), space);
    Ok((name, snap))
}

/// Scan `dir` into a one-space [`Snapshot`] (the name is discarded — used by the
/// watcher, which already knows the directory it is re-scanning).
pub fn load_space(dir: &Path) -> Result<Snapshot, ScanError> {
    Ok(scan_named(dir)?.1)
}

/// Build a one-artifact snapshot from a single file's HTML (the `create` model).
/// The lone artifact is the space's home (`SINGLE_SLUG`); its title is resolved
/// from the HTML (`<title>`/`<h1>`, parsed not regexed), falling back to the space
/// name. Fragment-vs-full-document detection is **not** done here: the raw HTML is
/// stored verbatim and the content route classifies + wraps it at serve time
/// (`wrap::render_artifact`), so `create` and `serve` share one detector.
pub fn one_artifact_snapshot(name: &str, html: String) -> Snapshot {
    let title = space::resolve_title(&html).unwrap_or_else(|| name.to_string());
    let mut sp = Space::default();
    sp.artifacts
        .insert(SINGLE_SLUG.to_string(), Artifact { html, title });
    sp.nav = vec![SINGLE_SLUG.to_string()];
    sp.home = Some(SINGLE_SLUG.to_string());
    let mut snap = Snapshot::empty();
    snap.spaces.insert(name.to_string(), sp);
    snap
}

/// A single file's `(len, mtime_nanos)` change fingerprint (see [`file_fp`]).
type FileFp = (u64, i128);

/// The combined change fingerprint of a `render` session's source(s): the markdown
/// file plus, for a file template, the template file. Either changing re-renders.
type RenderFp = (FileFp, Option<FileFp>);

/// The template a `render` session re-applies on every (re)render. A built-in is a
/// static fragment; a file is re-read each render so editing it reloads the browser
/// (the same live-edit loop `serve`/`create` give a directory / single file).
/// `Clone` is a pointer copy for `Builtin(&'static str)` and a `PathBuf` allocation
/// for `File`, cloned into the watcher's `spawn_blocking` closures.
#[derive(Clone)]
pub enum RenderTemplate {
    Builtin(&'static str),
    File(PathBuf),
}

impl RenderTemplate {
    /// The template file to also watch, if the template is a local file.
    fn file_path(&self) -> Option<&Path> {
        match self {
            RenderTemplate::Builtin(_) => None,
            RenderTemplate::File(p) => Some(p),
        }
    }

    /// Re-read (for a file template) the current template string.
    fn read(&self) -> Result<String, String> {
        match self {
            RenderTemplate::Builtin(s) => Ok((*s).to_string()),
            RenderTemplate::File(p) => read_artifact_file(p),
        }
    }
}

/// Cap the rendered artifact body so a `render` artifact obeys the same per-file
/// resource bound the directory scanner and `create` enforce (`MAX_FILE_BYTES`).
/// Markdown/template *inputs* are each capped at that limit, but rendering can
/// amplify markup, so the generated body is checked too — otherwise a `render`d
/// artifact could exceed the space model's per-artifact invariant. Returns the
/// over-limit message on failure so both the initial CLI load (fatal) and the
/// watcher (keep last-good) can report it.
pub fn enforce_body_cap(body: String) -> Result<String, String> {
    if body.len() as u64 > space::MAX_FILE_BYTES {
        return Err(format!(
            "rendered output is {} bytes, over the {}-byte per-artifact limit",
            body.len(),
            space::MAX_FILE_BYTES
        ));
    }
    Ok(body)
}

/// Render `md_path` (markdown) through `template` into an artifact body, bounded by
/// [`enforce_body_cap`]. Used by both the initial `render` load and the watcher
/// reload, so the two share one renderer. Returns an informative message on failure
/// (a bad read, a template missing/duplicating `{{content}}`, or an over-limit
/// rendered body) — the caller decides fatal-vs-log.
pub fn build_render_body(md_path: &Path, template: &RenderTemplate) -> Result<String, String> {
    let md = read_artifact_file(md_path)?;
    let tstr = template.read()?;
    let body = render::render_to_body(&md, &tstr).map_err(|e| e.to_string())?;
    enforce_body_cap(body)
}

/// The `render` analogue of [`spawn_file_watcher`]: poll the markdown file **and**
/// (for a file template) the template file, and on a change to either, re-render
/// into a fresh one-artifact snapshot, swap atomically, and fire the SSE reload. A
/// render that fails (a removed/oversize/non-UTF-8 source, an over-limit rendered
/// body, or a template that lost its `{{content}}` mid-edit) keeps the last-good
/// snapshot serving and is logged once, so a transient bad save never blanks the
/// page. A persistently-failing source state is attempted **once** (not re-rendered
/// every tick): `last_err_fp` gates the work, not just the log, so an invalid 8 MiB
/// template does not re-parse at 2 Hz until the next edit changes the fingerprint.
pub fn spawn_render_watcher(
    host: Arc<ArtifactHost>,
    md_path: PathBuf,
    template: RenderTemplate,
    name: String,
) {
    tokio::spawn(async move {
        let tpath = template.file_path().map(Path::to_path_buf);
        let mut loaded_fp = render_fp_blocking(md_path.clone(), tpath.clone()).await;
        let mut last_err_fp: Option<RenderFp> = None;
        loop {
            tokio::time::sleep(WATCH_INTERVAL).await;
            let fp = render_fp_blocking(md_path.clone(), tpath.clone()).await;
            // Skip an unchanged good state AND a state we already tried and failed —
            // the latter won't succeed without a further edit (which moves the fp).
            if fp == loaded_fp || last_err_fp == Some(fp) {
                continue;
            }
            let (md, tpl) = (md_path.clone(), template.clone());
            match tokio::task::spawn_blocking(move || build_render_body(&md, &tpl)).await {
                Ok(Ok(body)) => {
                    host.swap(one_artifact_snapshot(&name, body));
                    host.notify_reload();
                    loaded_fp = fp;
                    last_err_fp = None;
                    eprintln!("glasspad: re-rendered {}", md_path.display());
                }
                Ok(Err(e)) => {
                    // The top-of-loop guard already skips a repeat of this fp, so this
                    // logs exactly once per distinct failing state, then waits for the
                    // next edit rather than re-rendering the same failure every tick.
                    eprintln!(
                        "glasspad: re-render of {} failed, keeping last good content: {e}",
                        md_path.display()
                    );
                    last_err_fp = Some(fp);
                }
                Err(join) => eprintln!("glasspad: render watcher task failed: {join}"),
            }
        }
    });
}

/// Fingerprint the render source(s): the markdown file's `(len, mtime)` plus, for a
/// file template, the template file's — so a change to either re-renders.
async fn render_fp_blocking(md: PathBuf, tpl: Option<PathBuf>) -> RenderFp {
    tokio::task::spawn_blocking(move || (file_fp(&md), tpl.map(|p| file_fp(&p))))
        .await
        .unwrap_or(((0, -1), None))
}

/// A dependency-free filesystem watcher: poll a cheap fingerprint of the scan
/// surface and, on change, rescan into a fresh snapshot, swap it atomically, and
/// fire the SSE reload. Runs the blocking fingerprint + scan on a blocking pool
/// (never an async worker). A rescan that fails (e.g. the user just introduced a
/// slug collision) keeps the last-good snapshot serving and is retried when the
/// surface changes again; the same failure is logged only once.
pub fn spawn_watcher(host: Arc<ArtifactHost>, dir: PathBuf) {
    tokio::spawn(async move {
        // `loaded_fp` tracks the last *successfully loaded* state, so a failed
        // scan is retried on the next tick instead of being silently skipped.
        let mut loaded_fp = fp_blocking(dir.clone()).await;
        let mut last_err_fp: Option<u64> = None;
        loop {
            tokio::time::sleep(WATCH_INTERVAL).await;
            let fp = fp_blocking(dir.clone()).await;
            if fp == loaded_fp {
                continue;
            }
            let d = dir.clone();
            match tokio::task::spawn_blocking(move || load_space(&d)).await {
                Ok(Ok(snap)) => {
                    host.swap(snap);
                    host.notify_reload();
                    loaded_fp = fp;
                    last_err_fp = None;
                    eprintln!("glasspad: reloaded {}", dir.display());
                }
                Ok(Err(e)) => {
                    // Keep serving the last-good snapshot; retry when the surface
                    // changes. Log each distinct failing state once (no 2 Hz spam).
                    if last_err_fp != Some(fp) {
                        eprintln!(
                            "glasspad: rescan of {} failed, keeping last good snapshot: {e}",
                            dir.display()
                        );
                        last_err_fp = Some(fp);
                    }
                }
                Err(join) => eprintln!("glasspad: watcher task failed: {join}"),
            }
        }
    });
}

/// The single-file analogue of [`spawn_watcher`] (the `create` model): poll one
/// file's `(len, mtime)` and, on change, re-read it into a fresh one-artifact
/// snapshot, swap atomically, and fire the SSE reload. A read that fails (file
/// removed, non-UTF-8, over the per-file cap) keeps the last-good snapshot serving
/// and is logged once, so a single-file edit loop reloads the browser just like
/// `serve ./dir` while a transient bad save never blanks the page.
pub fn spawn_file_watcher(host: Arc<ArtifactHost>, file: PathBuf, name: String) {
    tokio::spawn(async move {
        let mut loaded_fp = file_fp_blocking(file.clone()).await;
        let mut last_err_fp: Option<(u64, i128)> = None;
        loop {
            tokio::time::sleep(WATCH_INTERVAL).await;
            let fp = file_fp_blocking(file.clone()).await;
            if fp == loaded_fp {
                continue;
            }
            let f = file.clone();
            match tokio::task::spawn_blocking(move || read_artifact_file(&f)).await {
                Ok(Ok(html)) => {
                    host.swap(one_artifact_snapshot(&name, html));
                    host.notify_reload();
                    loaded_fp = fp;
                    last_err_fp = None;
                    eprintln!("glasspad: reloaded {}", file.display());
                }
                Ok(Err(e)) => {
                    if last_err_fp != Some(fp) {
                        eprintln!(
                            "glasspad: reload of {} failed, keeping last good content: {e}",
                            file.display()
                        );
                        last_err_fp = Some(fp);
                    }
                }
                Err(join) => eprintln!("glasspad: file watcher task failed: {join}"),
            }
        }
    });
}

/// Read a single artifact file for the `create` watcher: reject a non-regular
/// file or one over the per-file cap **before** reading it all, then require
/// UTF-8. Returns an informative message on failure (logged, not fatal — the
/// watcher keeps the last-good snapshot). The initial `create` load does its own
/// richer validation (see `cli`); this is the reload path.
pub fn read_artifact_file(file: &Path) -> Result<String, String> {
    use std::io::Read;
    let meta = std::fs::metadata(file).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err(format!("{} is not a regular file", file.display()));
    }
    if meta.len() > space::MAX_FILE_BYTES {
        return Err(format!(
            "{} bytes, over the {}-byte per-file limit",
            meta.len(),
            space::MAX_FILE_BYTES
        ));
    }
    // Bounded read (cap the allocation at limit + 1) so a file that grows past the
    // cap between the stat above and the read cannot force an unbounded buffer.
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    f.take(space::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > space::MAX_FILE_BYTES {
        return Err(format!(
            "over the {}-byte per-file limit",
            space::MAX_FILE_BYTES
        ));
    }
    String::from_utf8(bytes).map_err(|_| format!("{} is not valid UTF-8", file.display()))
}

/// Run [`file_fp`] on the blocking pool.
async fn file_fp_blocking(file: PathBuf) -> (u64, i128) {
    tokio::task::spawn_blocking(move || file_fp(&file))
        .await
        .unwrap_or((0, -1))
}

/// A single file's change fingerprint: `(len, mtime_nanos)`. Follows symlinks
/// (`metadata`, not `symlink_metadata`) — `create` serves the file the user named
/// even if it is a symlink to their own file, so the watch tracks the target.
fn file_fp(file: &Path) -> (u64, i128) {
    match std::fs::metadata(file) {
        Ok(m) => {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as i128)
                .unwrap_or(-1);
            (m.len(), mtime)
        }
        Err(_) => (0, -1),
    }
}

/// Run `fingerprint` on the blocking pool.
async fn fp_blocking(dir: PathBuf) -> u64 {
    tokio::task::spawn_blocking(move || fingerprint(&dir))
        .await
        .unwrap_or(0)
}

/// A cheap change-detection fingerprint over **exactly the scan surface**: the
/// top-level directory listing (so any added/removed/edited top-level file or the
/// manifest is caught) plus the `assets/` subtree recursively. It deliberately
/// does **not** descend into other subdirectories (`.git`, `node_modules`, build
/// output) — the scanner ignores them, so walking them every tick would be wasted
/// CPU. Never follows symlinks (a symlink's own metadata is hashed, so swapping a
/// file for a symlink is detected).
fn fingerprint(dir: &Path) -> u64 {
    let mut entries: Vec<(PathBuf, bool, u64, i128)> = Vec::new();
    collect_level(dir, false, &mut entries); // top level only
    collect_level(&dir.join(space::ASSETS_DIR), true, &mut entries); // assets subtree
    entries.sort();
    let mut hasher = DefaultHasher::new();
    entries.hash(&mut hasher);
    hasher.finish()
}

/// Collect `(path, is_symlink, len, mtime_nanos)` for one directory. When
/// `recurse` is set, descend into real subdirectories (used for `assets/`).
fn collect_level(dir: &Path, recurse: bool, out: &mut Vec<(PathBuf, bool, u64, i128)>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_symlink = meta.file_type().is_symlink();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i128)
            .unwrap_or(-1);
        out.push((path.clone(), is_symlink, meta.len(), mtime));
        if recurse && meta.is_dir() && !is_symlink {
            collect_level(&path, true, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::util::ServiceExt;

    fn app() -> Router {
        build_app(3000)
    }

    // --- loopback return channel -------------------------------------------

    fn tmp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "gp-lb-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A loopback app whose `myspace` holds one artifact and whose host carries a
    /// fresh submission store rooted at `root`.
    fn app_with_channel(root: &Path) -> Router {
        let store = crate::submissions::SubmissionStore::open(root).unwrap();
        let host = Arc::new(ArtifactHost::new(3000).with_submissions(store));
        host.swap(one_artifact_snapshot("myspace", "<h1>form</h1>".into()));
        build_app_with_host(guards::HostPolicy::loopback(3000), host)
    }

    fn lb_req(
        method: Method,
        uri: &str,
        origin: Option<&str>,
        body: Option<&str>,
    ) -> Request<Body> {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "127.0.0.1:3000");
        if let Some(o) = origin {
            b = b.header("origin", o);
        }
        if body.is_some() {
            b = b.header("content-type", "application/json");
        }
        b.body(
            body.map(|s| Body::from(s.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap()
    }

    async fn resp_json(app: &Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let r = app.clone().oneshot(req).await.unwrap();
        let status = r.status();
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        let j = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, j)
    }

    #[tokio::test]
    async fn loopback_submit_then_poll_round_trips() {
        let root = tmp_root("roundtrip");
        let app = app_with_channel(&root);
        // Same-origin submit → 201.
        let (status, _) = resp_json(
            &app,
            lb_req(
                Method::POST,
                "/myspace/_gp/submit",
                Some("http://127.0.0.1:3000"),
                Some(r#"{"data":{"choice":"b"}}"#),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        // Poll it back.
        let (status, j) = resp_json(
            &app,
            lb_req(Method::GET, "/myspace/_gp/submissions", None, None),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(j["submissions"].as_array().unwrap().len(), 1);
        assert_eq!(j["submissions"][0]["data"]["choice"], "b");
        assert_eq!(j["submissions"][0]["artifact"], "index");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn loopback_stream_opens_event_stream_and_503_without_store() {
        // A2 SSE (loopback parity): a valid space opens a held `text/event-stream`
        // (200); a server with NO return channel answers 503, never a panic. The held
        // body is not read (it stays open) — only the status + content-type matter.
        let root = tmp_root("stream");
        let chan_app = app_with_channel(&root);
        let r = chan_app
            .clone()
            .oneshot(lb_req(
                Method::GET,
                "/myspace/_gp/submissions/stream",
                None,
                None,
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(
            r.headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("text/event-stream")
        );

        // The default test app carries no submission store → 503.
        let (status, j) = resp_json(
            &app(),
            lb_req(Method::GET, "/demo/_gp/submissions/stream", None, None),
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(j["error"]["code"], "return_channel_unavailable");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn loopback_submit_rejects_foreign_origin() {
        let root = tmp_root("csrf");
        let app = app_with_channel(&root);
        let (status, j) = resp_json(
            &app,
            lb_req(
                Method::POST,
                "/myspace/_gp/submit",
                Some("http://evil.example"),
                Some(r#"{"data":{}}"#),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(j["error"]["code"], "bad_origin");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn loopback_submit_rejects_missing_origin() {
        // Fail-closed CSRF: the trusted shell's fetch always sends Origin, so a
        // request with NO Origin is rejected rather than assumed same-origin.
        let root = tmp_root("noorigin");
        let app = app_with_channel(&root);
        let (status, j) = resp_json(
            &app,
            lb_req(
                Method::POST,
                "/myspace/_gp/submit",
                None,
                Some(r#"{"data":{}}"#),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(j["error"]["code"], "bad_origin");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn loopback_submit_rejects_stale_version() {
        let root = tmp_root("ver");
        let app = app_with_channel(&root);
        let (status, j) = resp_json(
            &app,
            lb_req(
                Method::POST,
                "/myspace/_gp/submit",
                Some("http://localhost:3000"),
                Some(r#"{"data":{},"content_version":"00000000deadbeef"}"#),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(j["error"]["code"], "content_version_mismatch");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn loopback_submit_disabled_without_store_is_503() {
        // The default test app has NO submission store; submit answers 503, never a
        // panic or a silent accept.
        let (status, j) = resp_json(
            &app(),
            lb_req(
                Method::POST,
                "/demo/_gp/submit",
                Some("http://127.0.0.1:3000"),
                Some(r#"{"data":{}}"#),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(j["error"]["code"], "return_channel_unavailable");
    }

    #[test]
    fn enforce_body_cap_bounds_rendered_output() {
        // A body at/under the per-artifact cap passes; over it is rejected so a
        // `render`d artifact never exceeds the `MAX_FILE_BYTES` invariant that the
        // scanner and `create` hold for on-disk files.
        let ok = "x".repeat(1024);
        assert_eq!(enforce_body_cap(ok.clone()).unwrap(), ok);
        let too_big = "x".repeat((space::MAX_FILE_BYTES + 1) as usize);
        let err = enforce_body_cap(too_big).unwrap_err();
        assert!(err.contains("over the"), "msg: {err}");
    }

    #[test]
    fn build_render_body_enforces_output_cap() {
        // A template that balloons the body past the cap fails closed (kept last-good
        // by the watcher; a fatal `rendered_output_too_large` on the initial load).
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("gp-render-cap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("doc.md");
        std::fs::File::create(&md)
            .unwrap()
            .write_all(b"# hi\n")
            .unwrap();
        // Leak a huge static template string to exercise the Builtin path's cap.
        let big: &'static str = Box::leak(
            format!(
                "{}{{{{content}}}}",
                "y".repeat((space::MAX_FILE_BYTES + 16) as usize)
            )
            .into_boxed_str(),
        );
        let err = build_render_body(&md, &RenderTemplate::Builtin(big)).unwrap_err();
        assert!(err.contains("over the"), "expected cap error, got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    async fn send(req: Request<Body>) -> StatusCode {
        app().oneshot(req).await.unwrap().status()
    }

    /// Wave 3 (D2) invariant: the legacy same-origin control surface is gone and
    /// no *new* same-origin mutation endpoint has crept back in. This is what
    /// makes it safe to have unwired `control_origin_guard` — there is nothing
    /// state-mutating for it to protect. If a future wave adds a `POST`/`PUT`/
    /// `DELETE` control route, this test fails until Origin protection is wired,
    /// forcing the guard back on before the endpoint ships.
    #[tokio::test]
    async fn no_mutating_same_origin_surface_exists() {
        // Neither the removed legacy `/api/pads` CRUD surface nor any mutating
        // method against a live artifact route is *handled*: every one bounces
        // with 404 (no such route) or 405 (the artifact routes are GET-only, so
        // `/api/pads` now merely falls through to `/{space}/{slug}` and a
        // mutating verb is rejected). A 2xx here would mean a same-origin write
        // path exists — the thing the unwired `control_origin_guard` would need
        // to protect. The invariant that keeps it safely unwired is: there is
        // none.
        let cases = [
            (Method::GET, "/api/pads"),
            (Method::POST, "/api/pads"),
            (Method::PUT, "/api/pads/abc"),
            (Method::DELETE, "/api/pads/abc"),
            (Method::POST, "/demo/_c/index"),
            (Method::PUT, "/demo/_c/index"),
            (Method::DELETE, "/demo/_c/index"),
            (Method::PATCH, "/demo/_c/index"),
        ];
        for (method, uri) in cases {
            let s = send(
                Request::builder()
                    .method(method.clone())
                    .uri(uri)
                    .header("host", "127.0.0.1:3000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert!(
                matches!(s, StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED),
                "{method} {uri} was handled ({s}) — an unguarded same-origin \
                 mutation surface may have been (re)introduced"
            );
        }
    }

    #[tokio::test]
    async fn host_guard_accepts_loopback() {
        let s = send(
            Request::get("/demo/_c/index")
                .header("host", "127.0.0.1:3000")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let s = send(
            Request::get("/demo/_c/index")
                .header("host", "localhost:3000")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }

    /// `loopback serve --bind <LAN-IP>` (loopback-lan-serve): a LAN-bound app answers
    /// the opted-in LAN Host AND loopback, but STILL refuses every other Host — the
    /// DNS-rebinding guard is loosened by exactly one allowlist entry, not disabled.
    #[tokio::test]
    async fn lan_bound_app_allowlists_only_the_configured_host() {
        let policy = guards::HostPolicy {
            port: 3000,
            allow_host: Some("192.168.1.50".into()),
        };
        let app = build_app_with_host(policy, Arc::new(ArtifactHost::new(3000)));
        let get = |host: &str| {
            let host = host.to_string();
            let app = app.clone();
            async move {
                app.oneshot(
                    Request::get("/demo/_c/index")
                        .header("host", host)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
            }
        };
        // The opted-in LAN host + loopback are served.
        assert_eq!(get("192.168.1.50:3000").await, StatusCode::OK);
        assert_eq!(get("192.168.1.50").await, StatusCode::OK);
        assert_eq!(get("127.0.0.1:3000").await, StatusCode::OK);
        assert_eq!(get("localhost:3000").await, StatusCode::OK);
        // A DNS-rebinding attacker Host, a different LAN IP, and a foreign port are
        // all still refused — the guard is an allowlist, not an off switch.
        assert_eq!(
            get("attacker.example.com").await,
            StatusCode::MISDIRECTED_REQUEST
        );
        assert_eq!(
            get("192.168.1.99:3000").await,
            StatusCode::MISDIRECTED_REQUEST
        );
        assert_eq!(
            get("192.168.1.50:9999").await,
            StatusCode::MISDIRECTED_REQUEST
        );
    }

    #[test]
    fn resolve_lan_exposure_accepts_ip_and_rejects_wildcard_loopback_and_junk() {
        // A concrete LAN IP resolves: it binds itself and is the allowlist host + origin.
        let lan = resolve_lan_exposure("192.168.1.50", 3000).expect("LAN IP resolves");
        assert_eq!(lan.allow_host, "192.168.1.50");
        assert_eq!(lan.origin, "http://192.168.1.50:3000");
        assert!(
            lan.addrs
                .iter()
                .any(|a| a.to_string() == "192.168.1.50:3000")
        );

        // The wildcard, loopback, and malformed values are all hard errors — never a
        // silent public bind.
        assert!(resolve_lan_exposure("0.0.0.0", 3000).is_err());
        assert!(resolve_lan_exposure("::", 3000).is_err());
        assert!(resolve_lan_exposure("127.0.0.1", 3000).is_err());
        assert!(resolve_lan_exposure("localhost", 3000).is_err()); // resolves only to loopback
        assert!(resolve_lan_exposure("http://192.168.1.50", 3000).is_err()); // URL, not a host
        assert!(resolve_lan_exposure("192.168.1.50:8080", 3000).is_err()); // embedded port
        assert!(resolve_lan_exposure("   ", 3000).is_err()); // empty
    }

    #[tokio::test]
    async fn host_guard_rejects_rebinding_and_missing() {
        // DNS-rebinding attacker Host.
        let s = send(
            Request::get("/demo/_c/index")
                .header("host", "attacker.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::MISDIRECTED_REQUEST);
        // Foreign port.
        let s = send(
            Request::get("/demo/_c/index")
                .header("host", "127.0.0.1:9999")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::MISDIRECTED_REQUEST);
    }
}
