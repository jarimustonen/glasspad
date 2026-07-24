//! v0.2 HTML-artifact host — **Wave 1 security gate + Wave 2a space model**.
//!
//! Stands up the null-origin sandboxed iframe host and its security contract
//! (Wave 1), and on top of it the **space model + live directory serving**
//! (Wave 2a): a directory of files becomes a live, safely-served space. See
//! `issues/html-artifact-host-rewrite/{design,plan,wave-plan}.md`.
//!
//! URL topology (design.md §/plan.md §3):
//! * `GET /{space}/`               — space entry (trusted shell for the home artifact)
//! * `GET /{space}/{slug}`         — trusted shell framing the artifact
//! * `GET /{space}/_c/{slug}`      — raw artifact document (carries the sandbox CSP)
//! * `GET /{space}/assets/{*path}` — a space's static assets (MIME + size limits)
//! * `GET /_gp/reload`             — SSE stream that fires on a filesystem change
//! * `GET /_gp/v1/*`              — pinned base libraries (Wave 2b fills content)
//!
//! Wave 2a serves two kinds of space uniformly through one set of handlers:
//! * a **live directory snapshot** (`space::Snapshot`, swapped atomically on
//!   rescan), and
//! * the built-in **`demo` fixtures** — the deliberately-hostile Wave 1 probes,
//!   kept as the security regression suite. A request resolves against the live
//!   snapshot first; only spaces absent from it fall back to the fixtures.

pub mod fixtures;
pub mod guards;
pub mod headers;
pub mod shell;
pub mod space;

use std::convert::Infallible;
use std::sync::{Arc, RwLock};

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Response,
    },
    routing::get,
    Router,
};
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::security::token;
use space::Snapshot;

/// Shared state for the artifact host: the port (the CSP must name the explicit
/// host), the live directory **snapshot** (immutable, swapped atomically on
/// rescan so a half-written file is never served), and a broadcast channel the
/// filesystem watcher pokes to drive SSE reloads.
pub struct ArtifactHost {
    pub port: u16,
    snapshot: RwLock<Arc<Snapshot>>,
    reload_tx: broadcast::Sender<()>,
}

impl ArtifactHost {
    pub fn new(port: u16) -> Self {
        let (reload_tx, _) = broadcast::channel(16);
        Self {
            port,
            snapshot: RwLock::new(Arc::new(Snapshot::empty())),
            reload_tx,
        }
    }

    /// Cheap, lock-brief read of the current immutable snapshot.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.read().expect("snapshot lock poisoned").clone()
    }

    /// Atomically install a freshly-built snapshot. Readers in flight keep the
    /// old `Arc`; new readers see the new one — never a half-built mix.
    pub fn swap(&self, snap: Snapshot) {
        *self.snapshot.write().expect("snapshot lock poisoned") = Arc::new(snap);
    }

    /// Subscribe to reload notifications (one receiver per open SSE connection).
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.reload_tx.subscribe()
    }

    /// Fire a reload to every connected SSE client (called after an atomic swap).
    pub fn notify_reload(&self) {
        let _ = self.reload_tx.send(());
    }
}

/// Build the artifact-host router over a shared [`ArtifactHost`]. Finalized to
/// `Router<()>` so it merges into the main server router. Control-plane guards
/// (§5) are applied by the caller in `server::run` across the whole app.
pub fn router(host: Arc<ArtifactHost>) -> Router {
    Router::new()
        .route("/{space}/", get(space_entry))
        .route("/{space}/_c/{slug}", get(artifact_content))
        .route("/{space}/assets/{*path}", get(space_asset))
        .route("/{space}/{slug}", get(shell_page))
        .route("/_gp/reload", get(reload_stream))
        .route("/_gp/v1/{*path}", get(gp_asset))
        .with_state(host)
}

// --- Path/slug validation -------------------------------------------------

/// Canonical slug/space grammar. Lowercase, digit, hyphen; must start with an
/// alphanumeric; bounded length. Reserved names are rejected separately.
fn valid_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    bytes
        .iter()
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub const RESERVED: &[&str] = &["_gp", "_c", "assets", "api"];

fn valid_space(s: &str) -> bool {
    valid_name(s) && !RESERVED.contains(&s)
}

// --- Live-space + fixtures resolution -------------------------------------

/// A resolved artifact: its raw HTML plus (for live spaces) the parsed title.
struct Hit {
    html: String,
    title: Option<String>,
}

/// Resolve an artifact. A space present in the live snapshot is served **only**
/// from it (a real space never leaks the built-in `demo` probes); spaces absent
/// from the snapshot fall back to the fixtures registry.
fn find_artifact(host: &ArtifactHost, space: &str, slug: &str) -> Option<Hit> {
    let snap = host.snapshot();
    if let Some(sp) = snap.space(space) {
        return sp.artifact(slug).map(|a| Hit {
            html: a.html.clone(),
            title: Some(a.title.clone()),
        });
    }
    fixtures::get(space, slug).map(|f| Hit {
        html: f.html.to_string(),
        title: None,
    })
}

/// Ordered slugs for a space's nav (live nav order, else the fixtures order).
fn space_slugs(host: &ArtifactHost, space: &str) -> Vec<String> {
    let snap = host.snapshot();
    if let Some(sp) = snap.space(space) {
        return sp.slugs();
    }
    fixtures::slugs(space)
        .into_iter()
        .map(|s| s.to_string())
        .collect()
}

/// The home slug for a space (`index` > `home` > first in nav order).
fn space_home(host: &ArtifactHost, space: &str) -> Option<String> {
    let snap = host.snapshot();
    if let Some(sp) = snap.space(space) {
        return sp.home.clone();
    }
    let slugs = fixtures::slugs(space);
    if slugs.is_empty() {
        return None;
    }
    Some(if slugs.contains(&"index") {
        "index".to_string()
    } else {
        slugs[0].to_string()
    })
}

// --- Handlers -------------------------------------------------------------

#[derive(Deserialize, Default)]
struct ContentQuery {
    /// Diagnostic knob for the adversarial suite. `csp=noeval` serves the CSP
    /// **without** `'unsafe-eval'` — strictly tighter, never looser — so the
    /// suite can prove Vega-Lite's `new Function` truly needs it (design.md §4).
    /// Honored **only in debug builds**; release builds ignore it entirely so no
    /// query string can alter the frozen production policy.
    csp: Option<String>,
}

/// Raw artifact document. Carries the sandbox CSP so a direct-open is sandboxed
/// too, plus the egress CSP and the hardening headers.
async fn artifact_content(
    State(host): State<Arc<ArtifactHost>>,
    Path((space, slug)): Path<(String, String)>,
    Query(q): Query<ContentQuery>,
) -> Response {
    if !valid_space(&space) || !valid_name(&slug) {
        return not_found();
    }
    let Some(hit) = find_artifact(&host, &space, &slug) else {
        return not_found();
    };
    // The noeval knob only *tightens* the policy, and only in debug builds; the
    // `&& cfg!(...)` keeps `q` referenced in every profile (no unused warning).
    let allow_eval = !(cfg!(debug_assertions) && q.csp.as_deref() == Some("noeval"));
    let csp = headers::artifact_csp(host.port, allow_eval);

    let mut hmap = base_html_headers();
    hmap.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp).expect("csp is header-safe ascii"),
    );
    for (name, value) in headers::hardening_headers() {
        hmap.insert(name, value);
    }
    (hmap, Html(hit.html)).into_response()
}

/// Trusted shell framing the artifact.
async fn shell_page(
    State(host): State<Arc<ArtifactHost>>,
    Path((space, slug)): Path<(String, String)>,
) -> Response {
    if !valid_space(&space) || !valid_name(&slug) {
        return not_found();
    }
    let Some(hit) = find_artifact(&host, &space, &slug) else {
        return not_found();
    };
    render_shell(&host, &space, &slug, hit.title.as_deref())
}

/// Space entry — the shell for the home artifact (`index`, else first slug).
async fn space_entry(
    State(host): State<Arc<ArtifactHost>>,
    Path(space): Path<String>,
) -> Response {
    if !valid_space(&space) {
        return not_found();
    }
    let Some(home) = space_home(&host, &space) else {
        return not_found();
    };
    let title = find_artifact(&host, &space, &home).and_then(|h| h.title);
    render_shell(&host, &space, &home, title.as_deref())
}

fn render_shell(host: &ArtifactHost, space: &str, slug: &str, title: Option<&str>) -> Response {
    let nonce = token::generate_token();
    let slugs = space_slugs(host, space);
    let refs: Vec<&str> = slugs.iter().map(|s| s.as_str()).collect();
    let body = shell::render(space, slug, title.unwrap_or(""), &refs, &nonce);
    let csp = headers::shell_csp(&nonce);

    let mut hmap = base_html_headers();
    hmap.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp).expect("csp is header-safe ascii"),
    );
    // The shell must never be framed by anyone.
    hmap.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    for (name, value) in headers::hardening_headers() {
        hmap.insert(name, value);
    }
    (hmap, Html(body)).into_response()
}

/// A space's static asset (`/{space}/assets/{*path}`). Path-traversal is
/// structurally impossible: the request path is grammar-checked into a key, and
/// that key must exact-match the pre-scanned asset map — which only ever holds
/// real, symlink-vetted files under the space root. MIME is detected at scan
/// time; every response carries `nosniff` **and** a `sandbox` CSP, so a hostile
/// SVG/HTML asset opened *as a document* runs script-less in a null origin (the
/// `sandbox` directive doesn't apply to subresource loads, so JS/CSS/img/fonts
/// still load into an artifact).
async fn space_asset(
    State(host): State<Arc<ArtifactHost>>,
    Path((space, path)): Path<(String, String)>,
) -> Response {
    if !valid_space(&space) {
        return not_found();
    }
    let Some(key) = space::asset_key_for_request(&path) else {
        return not_found();
    };
    let snap = host.snapshot();
    let Some(sp) = snap.space(&space) else {
        return not_found();
    };
    let Some(asset) = sp.asset(&key) else {
        return not_found();
    };

    let mut hmap = HeaderMap::new();
    hmap.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(asset.content_type),
    );
    // Modules, fonts, and `fetch` from the null-origin artifact are cross-origin
    // to the asset host, so permit the read (no credentials); the asset is the
    // agent's own local content and holds no secret.
    hmap.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    // Neutralize a hostile top-level asset document; harmless for subresources.
    hmap.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox"),
    );
    hmap.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    for (name, value) in headers::hardening_headers() {
        hmap.insert(name, value);
    }
    (hmap, asset.bytes.clone()).into_response()
}

/// SSE reload stream. The trusted shell opens an `EventSource` to this path
/// (its `connect-src 'self'` permits it) and reloads when the filesystem watcher
/// fires after an atomic snapshot swap. The **artifact** CSP is widened to name
/// exactly this loopback path (see `headers::artifact_csp`) so a future in-frame
/// reload client can use it too — never to a foreign host, never a broad origin.
async fn reload_stream(
    State(host): State<Arc<ArtifactHost>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = host.subscribe();
    // A lag error still means "something changed" — surface it as a reload.
    let stream = BroadcastStream::new(rx)
        .map(|_| Ok::<Event, Infallible>(Event::default().event("reload").data("1")));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// `/_gp/v1/*` pinned base libraries. Wave 1 ships stubs; classic `<script src>`
/// / `<link>` don't need CORS, but the requests that are CORS-gated get
/// `Access-Control-Allow-Origin: *` (no credentials) per design.md §4.
async fn gp_asset(Path(path): Path<String>) -> Response {
    // Positive-filter path guard: a pinned asset is a single flat filename of
    // `[a-z0-9._-]`. Rejecting everything else (rather than blocklisting `..`/`/`)
    // closes encoded-traversal and any future normalization surprise up front;
    // the exact-match `gp_asset` lookup below is the real allowlist.
    if path.is_empty()
        || !path
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_'))
    {
        return not_found();
    }
    let Some((content_type, body)) = fixtures::gp_asset(&path) else {
        return not_found();
    };
    let mut hmap = HeaderMap::new();
    hmap.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    hmap.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    for (name, value) in headers::hardening_headers() {
        hmap.insert(name, value);
    }
    (hmap, body).into_response()
}

// --- helpers --------------------------------------------------------------

fn base_html_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    // Dynamic, security-sensitive documents (per-response nonce, live artifact
    // content): never cache, so a stale CSP/nonce or a `?csp` variant can't be
    // replayed for a later request.
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use space::{Artifact, Asset, Space};
    use tower::util::ServiceExt;

    fn empty_host() -> Arc<ArtifactHost> {
        Arc::new(ArtifactHost::new(3000))
    }

    async fn get_on(host: Arc<ArtifactHost>, path: &str) -> axum::http::Response<Body> {
        router(host)
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn get(path: &str) -> axum::http::Response<Body> {
        get_on(empty_host(), path).await
    }

    fn header<'a>(resp: &'a axum::http::Response<Body>, name: &str) -> &'a str {
        resp.headers()
            .get(name)
            .map(|v| v.to_str().unwrap())
            .unwrap_or("")
    }

    async fn body_string(resp: axum::http::Response<Body>) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    // --- grammar --------------------------------------------------------

    #[test]
    fn slug_grammar() {
        assert!(valid_name("index"));
        assert!(valid_name("sales-q3"));
        assert!(valid_name("a"));
        assert!(!valid_name(""));
        assert!(!valid_name("-lead"));
        assert!(!valid_name("Upper"));
        assert!(!valid_name("has space"));
        assert!(!valid_name("dot.ext"));
        assert!(!valid_name("../etc"));
        assert!(!valid_name(&"x".repeat(65)));
    }

    #[test]
    fn reserved_spaces_rejected() {
        assert!(!valid_space("_gp"));
        assert!(!valid_space("_c")); // also fails grammar (underscore), belt+braces
        assert!(!valid_space("api"));
        assert!(!valid_space("assets"));
        assert!(valid_space("demo"));
    }

    // --- Wave 1 security contract (demo fixtures still served) -----------

    #[tokio::test]
    async fn content_route_carries_full_security_contract() {
        let resp = get("/demo/_c/index").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let csp = header(&resp, "content-security-policy");
        assert!(csp.starts_with("sandbox allow-scripts"), "sandbox missing: {csp}");
        // Egress is bounded to exactly the loopback SSE-reload path — nothing else,
        // and never a foreign host. `/api/*` and canaries stay blocked.
        assert!(
            csp.contains("connect-src http://127.0.0.1:3000/_gp/reload http://localhost:3000/_gp/reload"),
            "connect-src not SSE-scoped: {csp}"
        );
        assert!(!csp.contains("connect-src *"), "connect-src wildcard: {csp}");
        assert!(csp.contains("http://127.0.0.1:3000"), "host not named: {csp}");
        assert!(csp.contains("'unsafe-eval'"), "eval frozen in: {csp}");
        assert_eq!(header(&resp, "x-content-type-options"), "nosniff");
        assert_eq!(header(&resp, "referrer-policy"), "no-referrer");
        assert!(header(&resp, "permissions-policy").contains("geolocation=()"));
    }

    #[tokio::test]
    async fn content_route_noeval_knob_only_tightens() {
        let resp = get("/demo/_c/eval?csp=noeval").await;
        let csp = header(&resp, "content-security-policy");
        assert!(!csp.contains("'unsafe-eval'"), "noeval must drop eval: {csp}");
        assert!(csp.starts_with("sandbox allow-scripts")); // still sandboxed
    }

    #[tokio::test]
    async fn shell_route_is_trusted_chrome_not_sandboxed() {
        let resp = get("/demo/index").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let csp = header(&resp, "content-security-policy");
        assert!(csp.contains("require-trusted-types-for 'script'"), "TT off: {csp}");
        assert!(!csp.contains("sandbox allow-scripts"), "shell must NOT self-sandbox");
        assert_eq!(header(&resp, "x-frame-options"), "DENY");
        let html = body_string(resp).await;
        assert!(html.contains(r#"sandbox="allow-scripts allow-top-navigation-by-user-activation""#));
        assert!(!html.contains("allow-same-origin"));
    }

    #[tokio::test]
    async fn space_entry_serves_home() {
        let resp = get("/demo/").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_string(resp).await.contains("/demo/_c/index"));
    }

    #[tokio::test]
    async fn gp_asset_stub_has_cors_and_nosniff() {
        let resp = get("/_gp/v1/charts.js").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(header(&resp, "access-control-allow-origin"), "*");
        assert_eq!(header(&resp, "x-content-type-options"), "nosniff");
        assert!(header(&resp, "content-type").contains("javascript"));
    }

    #[tokio::test]
    async fn reserved_and_bad_names_404() {
        assert_eq!(get("/api/index").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(get("/demo/_c/Bad%20Slug").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(get("/demo/nonexistent").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(get("/_gp/v1/../secret").await.status(), StatusCode::NOT_FOUND);
    }

    // --- Wave 2a: live space serving ------------------------------------

    fn host_with_space(name: &str, space: Space) -> Arc<ArtifactHost> {
        let host = Arc::new(ArtifactHost::new(3000));
        let mut snap = Snapshot::empty();
        snap.spaces.insert(name.to_string(), space);
        host.swap(snap);
        host
    }

    fn demo_like_space() -> Space {
        let mut s = Space::default();
        s.artifacts.insert(
            "index".to_string(),
            Artifact { html: "<!doctype html><h1>Live home</h1>".into(), title: "Live Home".into() },
        );
        s.artifacts.insert(
            "sales".to_string(),
            Artifact { html: "<!doctype html><h1>Sales</h1>".into(), title: "Sales".into() },
        );
        s.assets.insert(
            "assets/data.json".to_string(),
            Asset { content_type: "application/json; charset=utf-8", bytes: b"{\"ok\":true}".to_vec() },
        );
        s.nav = vec!["index".into(), "sales".into()];
        s.home = Some("index".into());
        s
    }

    #[tokio::test]
    async fn live_space_serves_its_own_artifacts_not_fixtures() {
        let host = host_with_space("myspace", demo_like_space());
        let resp = get_on(host.clone(), "/myspace/_c/index").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_string(resp).await.contains("Live home"));
        // A live space that lacks a slug 404s — it must NOT leak demo fixtures.
        assert_eq!(get_on(host, "/myspace/_c/exfil").await.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn live_shell_shows_resolved_title_as_text() {
        let host = host_with_space("myspace", demo_like_space());
        let html = body_string(get_on(host, "/myspace/sales").await).await;
        assert!(html.contains("Sales"));
        assert!(html.contains("/myspace/_c/sales"));
    }

    #[tokio::test]
    async fn asset_route_serves_with_mime_nosniff_and_sandbox() {
        let host = host_with_space("myspace", demo_like_space());
        let resp = get_on(host, "/myspace/assets/data.json").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(header(&resp, "content-type").contains("application/json"));
        assert_eq!(header(&resp, "x-content-type-options"), "nosniff");
        // A hostile top-level asset document is sandboxed (script-less null origin).
        assert_eq!(header(&resp, "content-security-policy"), "sandbox");
    }

    #[tokio::test]
    async fn asset_route_rejects_traversal_and_unknown() {
        let host = host_with_space("myspace", demo_like_space());
        // Real asset present.
        assert_eq!(get_on(host.clone(), "/myspace/assets/data.json").await.status(), StatusCode::OK);
        // Traversal attempts never resolve to a key in the pre-scanned map.
        assert_eq!(get_on(host.clone(), "/myspace/assets/..%2f..%2fetc%2fpasswd").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(get_on(host.clone(), "/myspace/assets/nope.json").await.status(), StatusCode::NOT_FOUND);
        // Unknown space → 404 (no fixtures have assets either).
        assert_eq!(get_on(host, "/demo/assets/x.js").await.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn atomic_swap_never_serves_a_partial_snapshot() {
        // Alternate between two complete snapshots from another thread while the
        // main thread reads; every read must be wholly one snapshot or the other.
        let host = Arc::new(ArtifactHost::new(3000));
        let mk = |body: &str| {
            let mut s = Space::default();
            s.artifacts.insert("index".into(), Artifact { html: body.to_string(), title: "t".into() });
            s.nav = vec!["index".into()];
            s.home = Some("index".into());
            let mut snap = Snapshot::empty();
            snap.spaces.insert("myspace".into(), s);
            snap
        };
        let a = "AAAAAAAA";
        let b = "BBBBBBBB";
        host.swap(mk(a));
        let writer = {
            let host = host.clone();
            let (a, b) = (a.to_string(), b.to_string());
            std::thread::spawn(move || {
                for i in 0..2000 {
                    host.swap(mk(if i % 2 == 0 { &a } else { &b }));
                }
            })
        };
        for _ in 0..2000 {
            let snap = host.snapshot();
            let html = &snap.space("myspace").unwrap().artifact("index").unwrap().html;
            assert!(html == a || html == b, "torn snapshot: {html:?}");
        }
        writer.join().unwrap();
    }

    #[tokio::test]
    async fn reload_endpoint_is_event_stream() {
        let resp = get("/_gp/reload").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(header(&resp, "content-type").contains("text/event-stream"));
    }
}
