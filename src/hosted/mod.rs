//! The **hosted share-server run mode** (0.3.0) — a long-lived, public-bind server
//! that many agents push pages to over the network (API-key auth) and that serves
//! them at unguessable capability-slug public URLs. It is a *separate run mode*
//! from the loopback `serve`/`create`/`render` path: it never binds loopback and
//! never uses `guards::host_guard`. See `issues/hosted-share-server/plan.md`.
//!
//! It reuses the **exact** rendering/sandbox seam (`artifact_host::{wrap, shell,
//! headers, space, render}`) — the published artifact flows through the same
//! content route under the same frozen CSP/sandbox/Trusted-Types headers; the only
//! parameterization is the CSP's named origin (public instead of loopback) and the
//! shell's `/p` URL mount (`ArtifactHost::new_public`). It adds its own **router,
//! storage, auth, and slug** layers on top (`auth`, `store`, `slug`, `ingest`).
//!
//! ## Ingest surfaces
//! * `POST /api/v1/pages` — one page → one single-artifact space (the original,
//!   immutable single-file publish; on-disk `pages/<slug>/`).
//! * `POST /api/v1/spaces` (Gap 1) — a whole **space** (a directory of linked
//!   `.html` artifacts) → one multi-artifact space under a single capability slug,
//!   served at `/p/<slug>/…` with the in-space bridge nav + relative links resolving
//!   across pages (the existing `spaces_router` read seam, unchanged). On-disk
//!   `spaces/<slug>/`; a stable `space_key` updates the space in place. Both surfaces
//!   are API-key + per-tenant scoped; the space read path stays a null-origin
//!   sandboxed iframe per page (no new grant, `connect-src 'none'`).
//!
//! ## Host handling (plan §8)
//! The loopback `host_guard` is a *rebinding* defense for a server a browser might
//! treat as privileged same-origin; it is **not** carried here (a public server
//! must serve its public host). The hosted security model does not *depend* on Host
//! validation: read is public-by-design (returns only the sandboxed artifact), write
//! requires a `Bearer` token in the `Authorization` header, and the Host header is
//! never reflected into a response (the CSP names the fixed `--public-host` origin;
//! the shell uses mount-relative URLs; the ingest `url` uses the configured origin).
//! As **defense-in-depth**, [`public_host_guard`] still rejects any request whose
//! `Host` is not the configured public host (except `/healthz`), so the server
//! answers only under its own name — closing host-confusion / cache-poisoning-if-
//! proxied without relying on the loopback rebinding mechanism.

pub mod auth;
pub mod ingest;
pub mod rounds;
pub mod slug;
pub mod store;
pub mod submit;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use axum::{
    Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::Duration;
use tokio::net::TcpListener;

use crate::artifact_host::space;
use crate::artifact_host::{self, ArtifactHost};
use crate::submissions::SubmissionStore;
use auth::KeyTable;
use store::Store;

/// The `/p` URL mount the hosted read routes live under (the shell emits
/// `{mount}/{slug}/…` links; `/_gp/*` stays at root).
pub const MOUNT: &str = "/p";

/// How often retention GC runs.
const GC_INTERVAL: StdDuration = StdDuration::from_secs(3600);

/// Router state shared by the ingest handler (the read handlers use the separate
/// `Arc<ArtifactHost>` state on the space/gp sub-routers).
#[derive(Clone)]
pub struct HostedState {
    pub store: Arc<Store>,
    /// The return-channel submission store (per-page persisted user input).
    pub submissions: Arc<SubmissionStore>,
    /// The canonical public origin (`scheme://host[:port]`) for returned URLs.
    pub public_origin: String,
    /// The URL mount for read routes (`/p`).
    pub mount: String,
}

/// Validated run configuration for the hosted server.
pub struct HostedConfig {
    pub bind: SocketAddr,
    pub public_origin: String,
    pub store_root: std::path::PathBuf,
    pub retention_days: i64,
}

/// Validate an operator-supplied `--public-host` value into a canonical origin
/// (`scheme://host[:port]`). Strict (AI-first §1): a bad scheme, an empty host,
/// userinfo, a path/query/fragment, or any character invalid in a URL (a space
/// would smuggle a second CSP source) is rejected with an informative message. A
/// bare trailing slash canonicalizes away. Parsed with a real URL parser because
/// the result is interpolated verbatim into the artifact CSP source list.
pub fn validate_public_origin(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    // Parse with a real URL parser so a value that is later interpolated verbatim
    // into the artifact CSP source list cannot smuggle a space (→ a second CSP
    // origin), userinfo, a path/query/fragment, or an invalid port past validation.
    let url =
        url::Url::parse(raw).map_err(|e| format!("public-host {raw:?} is not a valid URL: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!(
            "public-host scheme must be http or https, got {:?}",
            url.scheme()
        ));
    }
    if url.host().is_none() || url.host_str().is_none_or(str::is_empty) {
        return Err(format!("public-host {raw:?} has an empty host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!(
            "public-host {raw:?} must not contain userinfo (user:pass@)"
        ));
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err(format!(
            "public-host {raw:?} must be an origin only (scheme://host[:port]) — no path"
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(format!(
            "public-host {raw:?} must be an origin only — no query or fragment"
        ));
    }
    // `Url::origin().ascii_serialization()` is the canonical `scheme://host[:port]`
    // (default ports omitted), which is exactly what the CSP + returned URLs want.
    Ok(url.origin().ascii_serialization())
}

/// The canonical Host authority (`host` or `host:port`) the hosted server answers
/// under, derived from the validated public origin. Used by [`public_host_guard`].
/// Default ports (80/443) are omitted to match what browsers send in `Host`.
fn expected_authority(origin: &str) -> String {
    // `origin` is already `scheme://host[:port]` (from `validate_public_origin`).
    origin
        .split_once("://")
        .map(|(_, a)| a)
        .unwrap_or(origin)
        .to_ascii_lowercase()
}

/// Build the complete hosted-server router. `keys` gates the ingest route only;
/// read routes and `/_gp/*` are unauthenticated (public read by design).
pub fn build_router(state: HostedState, host: Arc<ArtifactHost>, keys: Arc<KeyTable>) -> Router {
    // Defense-in-depth Host allowlist (plan §8): the server answers only under its
    // configured public host, so a foreign/rebound Host is rejected on every route
    // except the liveness probe. This does not carry the loopback rebinding
    // mechanism; the hosted security model does not *depend* on it (module docs),
    // it just closes host-confusion / cache-poisoning-if-proxied.
    let expected = Arc::new(expected_authority(&state.public_origin));

    // Ingest: auth middleware on this route only, plus a body limit sized for the
    // largest artifact (JSON overhead slack on top of the per-file cap).
    let ingest_body_limit = space::MAX_FILE_BYTES as usize + 128 * 1024;
    let ingest = Router::new()
        .route("/api/v1/pages", post(ingest::publish))
        .route_layer(middleware::from_fn_with_state(
            keys.clone(),
            auth::ingest_auth,
        ))
        .layer(DefaultBodyLimit::max(ingest_body_limit))
        .with_state(state.clone());

    // Space ingest (Gap 1): a whole space bundle. Same auth, but a body limit sized
    // for a full space (per-space byte cap + base64 (~4/3) + JSON overhead slack).
    let space_body_limit = (space::MAX_SPACE_BYTES as usize) * 4 / 3
        + space::MAX_SPACE_BYTES as usize / 4
        + 1024 * 1024;
    let space_ingest = Router::new()
        .route("/api/v1/spaces", post(ingest::publish_space))
        .route_layer(middleware::from_fn_with_state(
            keys.clone(),
            auth::ingest_auth,
        ))
        .layer(DefaultBodyLimit::max(space_body_limit))
        .with_state(state.clone());

    // Read: space routes under /p, base libs at root. The shell emits /p/…
    // links via the host's mount so nested paths resolve. Hosted read responses
    // carry `X-Robots-Tag: noindex, nofollow` (below) — host-serve mode ONLY.
    let read = Router::new()
        .nest(MOUNT, artifact_host::spaces_router(host.clone()))
        .merge(artifact_host::gp_router(host))
        .layer(middleware::from_fn(add_noindex));

    // B2 multi-round: the owner re-renders a live page (API-key auth, owner-scoped).
    let rounds = rounds::router(state.clone(), keys.clone());

    // Return-channel routes: a public shell-callable submit + API-key-scoped reads.
    let submissions = submit::router(state, keys);

    Router::new()
        .route("/healthz", get(healthz))
        .merge(ingest)
        .merge(space_ingest)
        .merge(read)
        .merge(rounds)
        .merge(submissions)
        .layer(middleware::from_fn_with_state(expected, public_host_guard))
}

/// Stamp `X-Robots-Tag: noindex, nofollow` on every hosted read response so a
/// leaked capability URL is not indexed by a compliant crawler ("hold the link,
/// not indexed" — `skill.md` / the html-consolidation design G3). This is
/// advisory metadata, not an access-control boundary: the unguessable ~50-bit
/// slug is the real confidentiality mechanism. Purely **additive**:
/// the frozen CSP, `x-frame-options: DENY`, `referrer-policy: no-referrer`, and
/// `cache-control: no-store` set by the shared read handlers are left untouched.
/// **Host-serve mode ONLY** — the loopback `serve` router never carries this layer.
async fn add_noindex(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        "x-robots-tag",
        header::HeaderValue::from_static("noindex, nofollow"),
    );
    resp
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// Reject any request whose `Host` does not name the configured public host, so the
/// server answers only under its own name. `/healthz` is exempt (load balancers
/// probe it by IP). Fail-closed: a missing/foreign/malformed Host is rejected.
/// Comparison is case-insensitive and port-tolerant (a proxy may add/omit the
/// port), so it matches on the host label while still rejecting a foreign name.
async fn public_host_guard(
    State(expected): State<Arc<String>>,
    req: Request,
    next: Next,
) -> Response {
    if req.uri().path() == "/healthz" {
        return next.run(req).await;
    }
    let ok = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|h| host_matches(h, &expected));
    if ok {
        next.run(req).await
    } else {
        (StatusCode::MISDIRECTED_REQUEST, "bad Host header").into_response()
    }
}

/// Does the request `Host` name the configured authority? Exact (lowercased) match,
/// else a port-tolerant fallback comparing just the host labels — so a reverse
/// proxy that adds/omits the port still passes while a foreign hostname is rejected.
fn host_matches(request_host: &str, expected: &str) -> bool {
    let r = request_host.trim().to_ascii_lowercase();
    r == *expected || host_label(&r) == host_label(expected)
}

/// The host label of an `authority` (`host` or `host:port`), stripping a trailing
/// `:port`. IPv6-literal aware: a bracketed `[…]` host keeps its brackets and only
/// a port after the closing `]` is stripped.
fn host_label(authority: &str) -> &str {
    if let Some(close) = authority.rfind(']') {
        // IPv6 literal `[..]` optionally followed by `:port`.
        return &authority[..=close];
    }
    match authority.rsplit_once(':') {
        Some((host, _port)) => host,
        None => authority,
    }
}

/// Run the hosted share server until killed: open the store (loading existing
/// pages), bind the public address, spawn the retention-GC task, and serve.
/// Returns an error the CLI surfaces as its structured envelope (never panics).
pub async fn run(config: HostedConfig, keys: Arc<KeyTable>) -> Result<RunHandle, String> {
    let host = Arc::new(ArtifactHost::new_public(
        config.public_origin.clone(),
        MOUNT.to_string(),
    ));
    let store = Arc::new(
        Store::open(&config.store_root, host.clone())
            .map_err(|e| format!("cannot open store {}: {e}", config.store_root.display()))?,
    );
    let submissions = SubmissionStore::open(&config.store_root.join("submissions"))
        .map_err(|e| format!("cannot open submission store: {e}"))?;

    // Run retention GC ONCE synchronously before serving, so pages already past
    // retention at startup are not served for up to an hour after a restart.
    let retention = Duration::days(config.retention_days);
    if let Err(e) = store.gc(retention) {
        eprintln!("glasspad host: initial GC failed: {e}");
    }
    if let Err(e) = submissions.gc(retention) {
        eprintln!("glasspad host: initial submission GC failed: {e}");
    }
    let pages = store.page_count();

    let listener = TcpListener::bind(config.bind)
        .await
        .map_err(|e| format!("cannot bind {}: {e}", config.bind))?;

    // Retention GC: hourly thereafter, off the async workers (fs on the blocking pool).
    spawn_gc(store.clone(), submissions.clone(), retention);

    let state = HostedState {
        store: store.clone(),
        submissions,
        public_origin: config.public_origin.clone(),
        mount: MOUNT.to_string(),
    };
    let app = build_router(state, host, keys);

    Ok(RunHandle {
        listener,
        app,
        pages,
    })
}

/// A bound-but-not-yet-serving hosted server: the CLI prints its startup envelope
/// from `pages`/local addr, then calls [`RunHandle::serve`] to block.
pub struct RunHandle {
    listener: TcpListener,
    app: Router,
    pub pages: usize,
}

impl RunHandle {
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Serve until the process is killed.
    pub async fn serve(self) -> std::io::Result<()> {
        axum::serve(self.listener, self.app).await
    }
}

/// Spawn the periodic retention-GC task (pages + return-channel submissions).
fn spawn_gc(store: Arc<Store>, submissions: Arc<SubmissionStore>, retention: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(GC_INTERVAL);
        // Skip the immediate first tick so startup isn't followed by an instant GC.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let store = store.clone();
            let subs = submissions.clone();
            match tokio::task::spawn_blocking(move || {
                let pages = store.gc(retention);
                let subs_removed = subs.gc(retention);
                (pages, subs_removed)
            })
            .await
            {
                Ok((pages, subs)) => {
                    match pages {
                        Ok(n) if n > 0 => {
                            eprintln!("glasspad host: GC removed {n} expired page(s)")
                        }
                        Ok(_) => {}
                        Err(e) => eprintln!("glasspad host: GC error: {e}"),
                    }
                    match subs {
                        Ok(n) if n > 0 => {
                            eprintln!("glasspad host: GC removed {n} expired submission(s)")
                        }
                        Ok(_) => {}
                        Err(e) => eprintln!("glasspad host: submission GC error: {e}"),
                    }
                }
                Err(e) => eprintln!("glasspad host: GC task panicked: {e}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::util::ServiceExt;

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "gp-hosted-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn app_with(root: &std::path::Path) -> (Router, Arc<ArtifactHost>, Arc<Store>) {
        let host = Arc::new(ArtifactHost::new_public(
            "https://pad.example.com".into(),
            MOUNT.to_string(),
        ));
        let store = Arc::new(Store::open(root, host.clone()).unwrap());
        let submissions = SubmissionStore::open(&root.join("submissions")).unwrap();
        let keys = Arc::new(KeyTable::parse(&format!("acme:{KEY}")).unwrap());
        let state = HostedState {
            store: store.clone(),
            submissions,
            public_origin: "https://pad.example.com".into(),
            mount: MOUNT.to_string(),
        };
        (build_router(state, host.clone(), keys), host, store)
    }

    async fn send(app: &Router, req: Request<Body>) -> axum::http::Response<Body> {
        app.clone().oneshot(req).await.unwrap()
    }

    async fn body_json(resp: axum::http::Response<Body>) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }

    /// The configured public host of the test app (`app_with` uses
    /// `https://pad.example.com`), so requests must carry it to pass the Host guard.
    const TEST_HOST: &str = "pad.example.com";

    fn publish_req(bearer: Option<&str>, json_body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/pages")
            .header("host", TEST_HOST)
            .header("content-type", "application/json");
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(json_body.to_string())).unwrap()
    }

    /// A GET carrying the configured Host so it passes the Host guard.
    fn get_req(uri: impl AsRef<str>) -> Request<Body> {
        Request::get(uri.as_ref())
            .header("host", TEST_HOST)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn noindex_covers_base_libs_and_matched_404s_but_not_healthz() {
        let root = tmp_root("noindex");
        let (app, _, _) = app_with(&root);

        // Base library at root carries noindex (a directly-shared asset URL).
        let r = send(&app, get_req("/_gp/v1/bridge.js")).await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(
            r.headers().get("x-robots-tag").unwrap().to_str().unwrap(),
            "noindex, nofollow"
        );

        // A 404 from a *matched* read handler still passes through the layer.
        let r = send(&app, get_req("/p/aaaaaaaaaaaaaaaaaaaaaaaaaa/_c/index")).await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            r.headers().get("x-robots-tag").unwrap().to_str().unwrap(),
            "noindex, nofollow"
        );

        // The liveness probe is not a hosted page — it must NOT carry noindex.
        let r = send(&app, get_req("/healthz")).await;
        assert_eq!(r.status(), StatusCode::OK);
        assert!(
            r.headers().get("x-robots-tag").is_none(),
            "healthz must not carry noindex"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn ingest_requires_valid_bearer_fail_closed() {
        let root = tmp_root("auth");
        let (app, _, _) = app_with(&root);
        let body = serde_json::json!({ "html": "<h1>hi</h1>" });

        // Missing header.
        let r = send(&app, publish_req(None, body.clone())).await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        // Empty bearer.
        let r = send(&app, publish_req(Some(""), body.clone())).await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        // Wrong key.
        let r = send(
            &app,
            publish_req(Some("wrong-but-long-enough-key-000000000"), body.clone()),
        )
        .await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        // Correct-prefix-but-wrong.
        let r = send(&app, publish_req(Some(&KEY[..31]), body.clone())).await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        // Valid.
        let r = send(&app, publish_req(Some(KEY), body)).await;
        assert_eq!(r.status(), StatusCode::CREATED);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn idempotency_key_replays_first_page_201_then_200() {
        let root = tmp_root("idem");
        let (app, _, _) = app_with(&root);

        // First publish with a key → 201 Created, mints a slug.
        let r = send(
            &app,
            publish_req(
                Some(KEY),
                serde_json::json!({ "html": "<h1>one</h1>", "idempotency_key": "digest-2026-08-09" }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let first_slug = body_json(r).await["slug"].as_str().unwrap().to_string();

        // Repeat with the same key → 200 OK, SAME slug (no new page).
        let r = send(
            &app,
            publish_req(
                Some(KEY),
                serde_json::json!({ "html": "<h1>different body</h1>", "idempotency_key": "digest-2026-08-09" }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK, "repeat must be 200, not 201");
        let again_slug = body_json(r).await["slug"].as_str().unwrap().to_string();
        assert_eq!(first_slug, again_slug, "repeat must return the first slug");

        // A different key → 201, fresh slug.
        let r = send(
            &app,
            publish_req(
                Some(KEY),
                serde_json::json!({ "html": "<h1>two</h1>", "idempotency_key": "digest-2026-08-10" }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let other_slug = body_json(r).await["slug"].as_str().unwrap().to_string();
        assert_ne!(
            first_slug, other_slug,
            "a distinct key must mint a fresh page"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn idempotency_key_over_length_is_rejected() {
        let root = tmp_root("idemlong");
        let (app, _, _) = app_with(&root);
        let long_key = "k".repeat(super::ingest::MAX_IDEMPOTENCY_KEY_CHARS + 1);
        let r = send(
            &app,
            publish_req(
                Some(KEY),
                serde_json::json!({ "html": "<h1>x</h1>", "idempotency_key": long_key }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        let j = body_json(r).await;
        assert_eq!(
            j["error"]["code"].as_str().unwrap(),
            "idempotency_key_too_long"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn misspelled_idempotency_key_field_is_rejected_not_silently_dropped() {
        // deny_unknown_fields guards the exactly-once contract: a camelCased/hyphenated
        // idempotency key must 400, not silently mint a fresh page every time.
        let root = tmp_root("idemunknown");
        let (app, _, _) = app_with(&root);
        let r = send(
            &app,
            publish_req(
                Some(KEY),
                serde_json::json!({ "html": "<h1>x</h1>", "idempotencyKey": "k" }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn empty_idempotency_key_is_rejected() {
        let root = tmp_root("idemempty");
        let (app, _, _) = app_with(&root);
        let r = send(
            &app,
            publish_req(
                Some(KEY),
                serde_json::json!({ "html": "<h1>x</h1>", "idempotency_key": "   " }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        let j = body_json(r).await;
        assert_eq!(
            j["error"]["code"].as_str().unwrap(),
            "idempotency_key_empty"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn published_page_is_served_sandboxed_under_public_origin() {
        let root = tmp_root("serve");
        let (app, _, _) = app_with(&root);
        let r = send(
            &app,
            publish_req(Some(KEY), serde_json::json!({ "html": "<h1>Report</h1>" })),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let j = body_json(r).await;
        let slug = j["slug"].as_str().unwrap().to_string();
        assert_eq!(
            j["url"].as_str().unwrap(),
            format!("https://pad.example.com/p/{slug}/")
        );

        // The content route carries the FROZEN artifact CSP, naming the public
        // origin — sandboxed, egress closed. Same seam as loopback, not widened.
        let r = send(&app, get_req(format!("/p/{slug}/_c/index"))).await;
        assert_eq!(r.status(), StatusCode::OK);
        let csp = r
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.starts_with("sandbox allow-scripts"), "csp: {csp}");
        assert!(csp.contains("connect-src 'none'"), "egress open: {csp}");
        assert!(
            csp.contains("https://pad.example.com"),
            "public origin unnamed: {csp}"
        );
        assert!(
            !csp.contains("127.0.0.1"),
            "loopback leaked into hosted csp: {csp}"
        );
        // Hosted content carries noindex so a leaked capability URL is not crawled.
        assert_eq!(
            r.headers().get("x-robots-tag").unwrap().to_str().unwrap(),
            "noindex, nofollow"
        );

        // The shell frames the /p-mounted content path.
        let r = send(&app, get_req(format!("/p/{slug}/"))).await;
        assert_eq!(r.status(), StatusCode::OK);
        // The shell route carries noindex too (same "hold the link" contract).
        assert_eq!(
            r.headers().get("x-robots-tag").unwrap().to_str().unwrap(),
            "noindex, nofollow"
        );
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&bytes);
        assert!(
            html.contains(&format!("/p/{slug}/_c/index")),
            "shell mount wrong"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn interpage_link_renders_with_top_level_navigation_intent() {
        // Regression (issue hosted-interpage-link-refused): a hosted page whose body
        // links to ANOTHER hosted `/p/` page must serve its content route with
        // `<base target="_top">`, so clicking the link breaks out of the sandboxed
        // iframe (top-level nav, permitted by the `allow-top-navigation-by-user-
        // activation` sandbox flag) instead of navigating in-frame into a shell
        // served `x-frame-options: DENY` → Chrome's "refused to connect". The link is
        // a fragment body, so it flows through `wrap::render_artifact`. Isolation is
        // preserved: the frozen artifact CSP (sandbox, `connect-src 'none'`) is
        // unchanged (asserted by the sibling serve/hostile tests).
        let root = tmp_root("interpage");
        let (app, _, _) = app_with(&root);
        let body = "<p>See also <a href=\"https://pad.example.com/p/other0slug00000000000000/\">the deep dive »</a></p>";
        let r = send(
            &app,
            publish_req(Some(KEY), serde_json::json!({ "html": body })),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let slug = body_json(r).await["slug"].as_str().unwrap().to_string();

        let r = send(&app, get_req(format!("/p/{slug}/_c/index"))).await;
        assert_eq!(r.status(), StatusCode::OK);
        // The content iframe sandbox permits user-activated top navigation.
        let csp = r
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            csp.contains("allow-top-navigation-by-user-activation"),
            "sandbox must permit top navigation: {csp}"
        );
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&bytes);
        assert!(
            html.contains(r#"<base target="_top">"#),
            "inter-page link must render with top-level navigation intent"
        );
        // The link itself is unchanged — the fix is the default target, not the href.
        assert!(html.contains("https://pad.example.com/p/other0slug00000000000000/"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn hostile_body_cannot_widen_csp_on_hosted_route() {
        let root = tmp_root("hostile");
        let (app, _, _) = app_with(&root);
        let hostile = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src *; connect-src *\">\
                       <script>fetch('http://evil.example/x')</script><h1>x</h1></body></html>";
        let r = send(
            &app,
            publish_req(Some(KEY), serde_json::json!({ "html": hostile })),
        )
        .await;
        let slug = body_json(r).await["slug"].as_str().unwrap().to_string();
        let r = send(&app, get_req(format!("/p/{slug}/_c/index"))).await;
        let csp = r
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.starts_with("sandbox allow-scripts"));
        assert!(
            csp.contains("connect-src 'none'"),
            "body widened egress: {csp}"
        );
        assert!(!csp.contains("default-src *"), "meta widened header: {csp}");
        std::fs::remove_dir_all(&root).ok();
    }

    fn space_req(bearer: Option<&str>, json_body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/spaces")
            .header("host", TEST_HOST)
            .header("content-type", "application/json");
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(json_body.to_string())).unwrap()
    }

    #[tokio::test]
    async fn space_publish_serves_multipage_with_nav_assets_and_frozen_csp() {
        let root = tmp_root("space-serve");
        let (app, _, _) = app_with(&root);
        // logo.svg base64 of "<svg></svg>".
        let logo_b64 = "PHN2Zz48L3N2Zz4=";
        let r = send(
            &app,
            space_req(
                Some(KEY),
                serde_json::json!({
                    "pages": [
                        { "slug": "index", "html": "<title>Home</title><h1>Home</h1><a href=\"./guide\">guide</a>" },
                        { "slug": "guide", "html": "<h1>Guide</h1>" }
                    ],
                    "assets": [ { "path": "logo.svg", "content_base64": logo_b64 } ],
                    "nav": ["index", "guide"],
                    "title": "My Docs"
                }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let j = body_json(r).await;
        let slug = j["slug"].as_str().unwrap().to_string();
        assert_eq!(j["page_count"].as_u64().unwrap(), 2);
        assert!(j["created"].as_bool().unwrap());
        assert_eq!(
            j["url"].as_str().unwrap(),
            format!("https://pad.example.com/p/{slug}/")
        );

        // Both pages serve under the FROZEN artifact CSP (sandbox, egress closed).
        for page in ["index", "guide"] {
            let r = send(&app, get_req(format!("/p/{slug}/_c/{page}"))).await;
            assert_eq!(r.status(), StatusCode::OK, "page {page} not served");
            let csp = r
                .headers()
                .get("content-security-policy")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            assert!(csp.starts_with("sandbox allow-scripts"), "csp: {csp}");
            assert!(csp.contains("connect-src 'none'"), "egress open: {csp}");
            assert!(!csp.contains("allow-forms"), "airlock: {csp}");
        }

        // The shell lists both pages in its nav (built from the server-resolved table).
        let r = send(&app, get_req(format!("/p/{slug}/"))).await;
        assert_eq!(r.status(), StatusCode::OK);
        let html = String::from_utf8_lossy(
            &axum::body::to_bytes(r.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .into_owned();
        assert!(
            html.contains(&format!("/p/{slug}/_c/index")),
            "home content path"
        );

        // The asset serves.
        let r = send(&app, get_req(format!("/p/{slug}/assets/logo.svg"))).await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(
            r.headers().get("content-type").unwrap().to_str().unwrap(),
            "image/svg+xml"
        );

        // Cross-space isolation: a bogus space slug is an opaque 404; the guide slug is
        // only reachable UNDER this space, never as a top-level space of its own.
        let r = send(&app, get_req("/p/aaaaaaaaaaaaaaaaaaaaaaaaaa/_c/index")).await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        let r = send(&app, get_req("/p/guide/_c/index")).await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn space_key_updates_in_place_201_then_200() {
        let root = tmp_root("space-idem-http");
        let (app, _, _) = app_with(&root);
        let mk = |body: &str| {
            serde_json::json!({
                "pages": [ { "slug": "index", "html": body } ],
                "space_key": "docs-v1"
            })
        };
        let r = send(&app, space_req(Some(KEY), mk("<h1>V1</h1>"))).await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let slug1 = body_json(r).await["slug"].as_str().unwrap().to_string();

        let r = send(&app, space_req(Some(KEY), mk("<h1>V2</h1>"))).await;
        assert_eq!(
            r.status(),
            StatusCode::OK,
            "re-publish must be 200, not 201"
        );
        let j = body_json(r).await;
        assert_eq!(j["slug"].as_str().unwrap(), slug1, "same slug on update");
        assert!(!j["created"].as_bool().unwrap());

        // The served body now reflects V2.
        let r = send(&app, get_req(format!("/p/{slug1}/_c/index"))).await;
        let html = String::from_utf8_lossy(
            &axum::body::to_bytes(r.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .into_owned();
        assert!(
            html.contains("V2"),
            "in-place update did not swap the served body"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn space_ingest_requires_auth_and_rejects_bad_bundle() {
        let root = tmp_root("space-bad");
        let (app, _, _) = app_with(&root);
        // Unauthenticated → 401.
        let r = send(
            &app,
            space_req(
                None,
                serde_json::json!({ "pages": [ { "slug": "index", "html": "x" } ] }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        // Empty pages → 400 invalid_space.
        let r = send(
            &app,
            space_req(Some(KEY), serde_json::json!({ "pages": [] })),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body_json(r).await["error"]["code"], "invalid_space");
        // Reserved page slug → 400.
        let r = send(
            &app,
            space_req(
                Some(KEY),
                serde_json::json!({ "pages": [ { "slug": "api", "html": "x" } ] }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        // Bad base64 asset → 400.
        let r = send(
            &app,
            space_req(
                Some(KEY),
                serde_json::json!({
                    "pages": [ { "slug": "index", "html": "x" } ],
                    "assets": [ { "path": "a.png", "content_base64": "not base64!!!" } ]
                }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body_json(r).await["error"]["code"], "bad_asset_base64");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn space_ingest_hostile_page_cannot_widen_csp() {
        // A page in the bundle whose body tries to widen the CSP via <meta> stays
        // contained — the server-set frozen artifact CSP is authoritative.
        let root = tmp_root("space-hostile");
        let (app, _, _) = app_with(&root);
        let hostile = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src *; connect-src *\">\
                       <script>fetch('http://evil.example/x')</script><h1>x</h1>";
        let r = send(
            &app,
            space_req(
                Some(KEY),
                serde_json::json!({ "pages": [ { "slug": "index", "html": hostile } ] }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let slug = body_json(r).await["slug"].as_str().unwrap().to_string();
        let r = send(&app, get_req(format!("/p/{slug}/_c/index"))).await;
        let csp = r
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.starts_with("sandbox allow-scripts"));
        assert!(
            csp.contains("connect-src 'none'"),
            "body widened egress: {csp}"
        );
        assert!(!csp.contains("default-src *"), "meta widened header: {csp}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn markdown_ingest_renders_via_shared_path() {
        let root = tmp_root("md");
        let (app, _, _) = app_with(&root);
        let r = send(
            &app,
            publish_req(
                Some(KEY),
                serde_json::json!({ "markdown": "# Hello\n\nHi.", "template": "prose" }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let slug = body_json(r).await["slug"].as_str().unwrap().to_string();
        let r = send(&app, get_req(format!("/p/{slug}/_c/index"))).await;
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&bytes);
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains(r#"<article class="gp-prose">"#));
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn ingest_rejects_missing_and_conflicting_body() {
        let root = tmp_root("badbody");
        let (app, _, _) = app_with(&root);
        // Neither html nor markdown.
        let r = send(&app, publish_req(Some(KEY), serde_json::json!({}))).await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        // Both.
        let r = send(
            &app,
            publish_req(
                Some(KEY),
                serde_json::json!({ "html": "<h1>x</h1>", "markdown": "# y" }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn unknown_slug_404s() {
        let root = tmp_root("404");
        let (app, _, _) = app_with(&root);
        let r = send(&app, get_req("/p/aaaaaaaaaaaaaaaaaaaaaaaaaa/_c/index")).await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn client_supplied_slug_is_rejected_no_cross_tenant_overwrite() {
        // A tenant cannot target another's page: the ingest body has no slug/tenant
        // field, and `deny_unknown_fields` now *rejects* any client-supplied
        // `slug`/`tenant` outright (400) rather than silently ignoring it. Either way
        // no chosen "victim" slug is ever written — cross-tenant overwrite is
        // structurally impossible.
        let root = tmp_root("noslug");
        let (app, _, _) = app_with(&root);
        let r = send(
            &app,
            publish_req(
                Some(KEY),
                serde_json::json!({
                    "html": "<h1>x</h1>",
                    "slug": "victim",
                    "tenant": "someone-else"
                }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
        // The chosen slug was never created.
        let r = send(&app, get_req("/p/victim/_c/index")).await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn two_tenants_publish_independently() {
        // Two distinct keys → two tenants; each publish is an independent immutable
        // page under its own random slug. Neither can name or clobber the other's.
        let root = tmp_root("tenants");
        let key2 = "fedcba9876543210fedcba9876543210";
        let host = Arc::new(ArtifactHost::new_public(
            "https://pad.example.com".into(),
            MOUNT.to_string(),
        ));
        let store = Arc::new(Store::open(&root, host.clone()).unwrap());
        let submissions = SubmissionStore::open(&root.join("submissions")).unwrap();
        let keys = Arc::new(KeyTable::parse(&format!("acme:{KEY}\nglobex:{key2}")).unwrap());
        let state = HostedState {
            store: store.clone(),
            submissions,
            public_origin: "https://pad.example.com".into(),
            mount: MOUNT.to_string(),
        };
        let app = build_router(state, host, keys);

        let a = body_json(
            send(
                &app,
                publish_req(Some(KEY), serde_json::json!({ "html": "<h1>A</h1>" })),
            )
            .await,
        )
        .await["slug"]
            .as_str()
            .unwrap()
            .to_string();
        let b = body_json(
            send(
                &app,
                publish_req(Some(key2), serde_json::json!({ "html": "<h1>B</h1>" })),
            )
            .await,
        )
        .await["slug"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(a, b);
        assert_eq!(store.page_count(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn hosted_host_guard_rejects_foreign_accepts_configured() {
        // The hosted server's defense-in-depth Host allowlist answers ONLY under the
        // configured public host: a foreign/rebound Host is rejected, the configured
        // one is accepted. This is NOT the loopback DNS-rebinding guard — the two run
        // modes are distinct; `server::tests::host_guard_rejects_rebinding_and_missing`
        // proves the loopback guard is unchanged.
        let root = tmp_root("hostguard");
        let (app, _, _) = app_with(&root);
        let slug = body_json(
            send(
                &app,
                publish_req(Some(KEY), serde_json::json!({ "html": "<h1>x</h1>" })),
            )
            .await,
        )
        .await["slug"]
            .as_str()
            .unwrap()
            .to_string();
        // Foreign Host → rejected.
        let r = send(
            &app,
            Request::get(format!("/p/{slug}/_c/index"))
                .header("host", "attacker.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(r.status(), StatusCode::MISDIRECTED_REQUEST);
        // Missing Host → rejected (fail-closed).
        let r = send(
            &app,
            Request::get(format!("/p/{slug}/_c/index"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(r.status(), StatusCode::MISDIRECTED_REQUEST);
        // Configured Host → accepted.
        let r = send(&app, get_req(format!("/p/{slug}/_c/index"))).await;
        assert_eq!(r.status(), StatusCode::OK);
        // Health check is exempt (probed by IP), even with a foreign/absent Host.
        let r = send(&app, Request::get("/healthz").body(Body::empty()).unwrap()).await;
        assert_eq!(r.status(), StatusCode::OK);
        std::fs::remove_dir_all(&root).ok();
    }

    // --- return channel (submissions) ---------------------------------------

    fn submit_req(slug: &str, origin: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/pages/{slug}/submit"))
            .header("host", TEST_HOST)
            .header("content-type", "application/json");
        if let Some(o) = origin {
            b = b.header("origin", o);
        }
        b.body(Body::from(body.to_string())).unwrap()
    }

    fn read_req(uri: &str, bearer: Option<&str>) -> Request<Body> {
        let mut b = Request::get(uri).header("host", TEST_HOST);
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::empty()).unwrap()
    }

    async fn publish_slug(app: &Router, body: serde_json::Value) -> String {
        body_json(send(app, publish_req(Some(KEY), body)).await).await["slug"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn submit_then_owner_reads_but_other_tenant_cannot() {
        let root = tmp_root("subs-read");
        let key2 = "fedcba9876543210fedcba9876543210";
        let host = Arc::new(ArtifactHost::new_public(
            "https://pad.example.com".into(),
            MOUNT.to_string(),
        ));
        let store = Arc::new(Store::open(&root, host.clone()).unwrap());
        let submissions = SubmissionStore::open(&root.join("submissions")).unwrap();
        let keys = Arc::new(KeyTable::parse(&format!("acme:{KEY}\nglobex:{key2}")).unwrap());
        let state = HostedState {
            store: store.clone(),
            submissions,
            public_origin: "https://pad.example.com".into(),
            mount: MOUNT.to_string(),
        };
        let app = build_router(state, host, keys);

        // acme publishes a page.
        let slug = publish_slug(&app, serde_json::json!({ "html": "<h1>form</h1>" })).await;

        // A visitor (the shell) submits — public write, no API key, same-origin.
        let r = send(
            &app,
            submit_req(
                &slug,
                Some("https://pad.example.com"),
                serde_json::json!({ "data": { "answer": "yes" } }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);

        // The OWNER (acme) can read the submission back.
        let r = send(
            &app,
            read_req(&format!("/api/v1/pages/{slug}/submissions"), Some(KEY)),
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK);
        let j = body_json(r).await;
        assert_eq!(j["submissions"].as_array().unwrap().len(), 1);
        assert_eq!(j["submissions"][0]["data"]["answer"], "yes");
        assert!(j["cursor"].as_u64().unwrap() >= 1);

        // A DIFFERENT tenant (globex) cannot read acme's page submissions — 404.
        let r = send(
            &app,
            read_req(&format!("/api/v1/pages/{slug}/submissions"), Some(key2)),
        )
        .await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);

        // An unauthenticated read is rejected (reads require the owner's key).
        let r = send(
            &app,
            read_req(&format!("/api/v1/pages/{slug}/submissions"), None),
        )
        .await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn stream_requires_owner_key_and_scopes_by_tenant() {
        // A2 SSE: the stream carries the SAME API-key + per-tenant scope as the poll/
        // wait reads. Unauth → 401; a non-owner → an opaque 404 (no submission bytes);
        // the owner → a held `text/event-stream` (200). All decided before any stream
        // body is produced, so a denied caller never receives a submission frame.
        let root = tmp_root("subs-stream");
        let key2 = "fedcba9876543210fedcba9876543210";
        let host = Arc::new(ArtifactHost::new_public(
            "https://pad.example.com".into(),
            MOUNT.to_string(),
        ));
        let store = Arc::new(Store::open(&root, host.clone()).unwrap());
        let submissions = SubmissionStore::open(&root.join("submissions")).unwrap();
        let keys = Arc::new(KeyTable::parse(&format!("acme:{KEY}\nglobex:{key2}")).unwrap());
        let state = HostedState {
            store: store.clone(),
            submissions,
            public_origin: "https://pad.example.com".into(),
            mount: MOUNT.to_string(),
        };
        let app = build_router(state, host, keys);
        let slug = publish_slug(&app, serde_json::json!({ "html": "<h1>form</h1>" })).await;
        let uri = format!("/api/v1/pages/{slug}/submissions/stream");

        // Unauthenticated → 401.
        let r = send(&app, read_req(&uri, None)).await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);

        // A different tenant (globex) streaming acme's page → opaque 404.
        let r = send(&app, read_req(&uri, Some(key2))).await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);

        // The owner → a held SSE stream (200, text/event-stream). Do NOT read the body
        // (it holds open); only the status + content-type are asserted.
        let r = send(&app, read_req(&uri, Some(KEY))).await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(
            r.headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("text/event-stream")
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn submit_rejects_foreign_origin_csrf() {
        let root = tmp_root("subs-csrf");
        let (app, _, _) = app_with(&root);
        let slug = publish_slug(&app, serde_json::json!({ "html": "<h1>x</h1>" })).await;
        // A cross-site page's fetch carries its own Origin → rejected.
        let r = send(
            &app,
            submit_req(
                &slug,
                Some("https://evil.example"),
                serde_json::json!({ "data": {} }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        assert_eq!(body_json(r).await["error"]["code"], "bad_origin");
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn submit_binds_slug_from_url_not_payload() {
        // Anti-spoof: a submission whose payload names another slug is still bound to
        // the URL-path slug (the payload `slug` field is ignored). The victim page
        // receives nothing.
        let root = tmp_root("subs-spoof");
        let (app, _, _) = app_with(&root);
        let victim = publish_slug(&app, serde_json::json!({ "html": "<h1>victim</h1>" })).await;
        let attacker = publish_slug(&app, serde_json::json!({ "html": "<h1>attacker</h1>" })).await;

        // Submit to the attacker's own page, but claim the victim's slug in the body.
        let r = send(
            &app,
            submit_req(
                &attacker,
                Some("https://pad.example.com"),
                serde_json::json!({ "data": { "x": 1 }, "slug": victim }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);

        // The victim page has NO submission (the body's slug was ignored).
        let r = send(
            &app,
            read_req(&format!("/api/v1/pages/{victim}/submissions"), Some(KEY)),
        )
        .await;
        assert_eq!(
            body_json(r).await["submissions"].as_array().unwrap().len(),
            0
        );
        // The attacker's own page has it.
        let r = send(
            &app,
            read_req(&format!("/api/v1/pages/{attacker}/submissions"), Some(KEY)),
        )
        .await;
        assert_eq!(
            body_json(r).await["submissions"].as_array().unwrap().len(),
            1
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn submit_rejects_stale_content_version() {
        let root = tmp_root("subs-version");
        let (app, _, _) = app_with(&root);
        let slug = publish_slug(&app, serde_json::json!({ "html": "<h1>v</h1>" })).await;
        // A wrong content-version echo is a cross-round mismatch → 409.
        let r = send(
            &app,
            submit_req(
                &slug,
                Some("https://pad.example.com"),
                serde_json::json!({ "data": {}, "content_version": "deadbeefdeadbeef" }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CONFLICT);
        assert_eq!(
            body_json(r).await["error"]["code"],
            "content_version_mismatch"
        );

        // The CORRECT (server-authoritative) version is accepted.
        let cv = crate::submissions::content_version("<h1>v</h1>");
        let r = send(
            &app,
            submit_req(
                &slug,
                Some("https://pad.example.com"),
                serde_json::json!({ "data": {}, "content_version": cv }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn submit_to_unknown_page_is_404() {
        let root = tmp_root("subs-404");
        let (app, _, _) = app_with(&root);
        let r = send(
            &app,
            submit_req(
                "aaaaaaaaaaaaaaaaaaaaaaaaaa",
                Some("https://pad.example.com"),
                serde_json::json!({ "data": {} }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(&root).ok();
    }

    // --- B2 multi-round (round push) ----------------------------------------

    fn round_req(slug: &str, bearer: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/pages/{slug}/rounds"))
            .header("host", TEST_HOST)
            .header("content-type", "application/json");
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).unwrap()
    }

    #[tokio::test]
    async fn round_push_swaps_served_body_under_frozen_csp_and_binds_new_round() {
        let root = tmp_root("round-serve");
        let (app, _, _) = app_with(&root);
        let slug = publish_slug(&app, serde_json::json!({ "html": "<h1>round zero</h1>" })).await;

        // Round 0 content-version, for the stale-round check later.
        let v0 = crate::submissions::content_version("<h1>round zero</h1>");

        // Owner pushes round 1.
        let r = send(
            &app,
            round_req(
                &slug,
                Some(KEY),
                serde_json::json!({ "html": "<h1>round one</h1>" }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK);
        let j = body_json(r).await;
        assert_eq!(j["round"].as_u64().unwrap(), 1);
        let v1 = j["content_version"].as_str().unwrap().to_string();
        assert_eq!(
            v1,
            crate::submissions::content_version("<h1>round one</h1>")
        );
        assert_ne!(v0, v1);

        // The content route now serves the new round — STILL under the frozen sandbox
        // CSP (pushing a round widens nothing).
        let r = send(&app, get_req(format!("/p/{slug}/_c/index"))).await;
        let csp = r
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(csp.starts_with("sandbox allow-scripts"), "csp: {csp}");
        assert!(csp.contains("connect-src 'none'"), "egress open: {csp}");
        assert!(
            !csp.contains("allow-forms"),
            "airlock: allow-forms crept in: {csp}"
        );
        let html = String::from_utf8_lossy(
            &axum::body::to_bytes(r.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .into_owned();
        assert!(html.contains("round one"), "new round body not served");

        // A submission answering the STALE round (v0) is rejected 409; the CURRENT
        // round (v1) is accepted — cross-round binding across a live re-render.
        let r = send(
            &app,
            submit_req(
                &slug,
                Some("https://pad.example.com"),
                serde_json::json!({ "data": {}, "content_version": v0 }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CONFLICT);
        let r = send(
            &app,
            submit_req(
                &slug,
                Some("https://pad.example.com"),
                serde_json::json!({ "data": {}, "content_version": v1 }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn round_push_is_owner_scoped_and_authenticated() {
        let root = tmp_root("round-auth");
        let key2 = "fedcba9876543210fedcba9876543210";
        let host = Arc::new(ArtifactHost::new_public(
            "https://pad.example.com".into(),
            MOUNT.to_string(),
        ));
        let store = Arc::new(Store::open(&root, host.clone()).unwrap());
        let submissions = SubmissionStore::open(&root.join("submissions")).unwrap();
        let keys = Arc::new(KeyTable::parse(&format!("acme:{KEY}\nglobex:{key2}")).unwrap());
        let state = HostedState {
            store: store.clone(),
            submissions,
            public_origin: "https://pad.example.com".into(),
            mount: MOUNT.to_string(),
        };
        let app = build_router(state, host, keys);

        let slug = publish_slug(
            &app,
            serde_json::json!({ "html": "<h1>acme owns this</h1>" }),
        )
        .await;

        // No bearer → 401.
        let r = send(
            &app,
            round_req(&slug, None, serde_json::json!({ "html": "<h1>x</h1>" })),
        )
        .await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);

        // A DIFFERENT tenant (globex) cannot re-render acme's page → opaque 404.
        let r = send(
            &app,
            round_req(
                &slug,
                Some(key2),
                serde_json::json!({ "html": "<h1>hijack</h1>" }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);

        // The victim's served body is unchanged.
        let r = send(&app, get_req(format!("/p/{slug}/_c/index"))).await;
        let html = String::from_utf8_lossy(
            &axum::body::to_bytes(r.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .into_owned();
        assert!(
            html.contains("acme owns this"),
            "non-owner push mutated the page"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn round_push_to_unknown_page_is_404() {
        let root = tmp_root("round-unknown");
        let (app, _, _) = app_with(&root);
        let r = send(
            &app,
            round_req(
                "aaaaaaaaaaaaaaaaaaaaaaaaaa",
                Some(KEY),
                serde_json::json!({ "html": "<h1>x</h1>" }),
            ),
        )
        .await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn validate_public_origin_accepts_and_rejects() {
        assert_eq!(
            validate_public_origin("https://pad.example.com").unwrap(),
            "https://pad.example.com"
        );
        assert_eq!(
            validate_public_origin("http://localhost:8080").unwrap(),
            "http://localhost:8080"
        );
        // A bare origin with a trailing slash canonicalizes to the origin (no path).
        assert_eq!(
            validate_public_origin("https://x.com/").unwrap(),
            "https://x.com"
        );
        assert!(validate_public_origin("pad.example.com").is_err()); // no scheme
        assert!(validate_public_origin("ftp://x").is_err()); // bad scheme
        assert!(validate_public_origin("https://").is_err()); // empty host
        assert!(validate_public_origin("https://x.com/path").is_err()); // path
        assert!(validate_public_origin("https://u:p@x.com").is_err()); // userinfo
        assert!(validate_public_origin("https://x.com?q=1").is_err()); // query
        // A space would smuggle a second CSP source — the URL parser rejects it.
        assert!(validate_public_origin("https://x.com evil.com").is_err());
    }

    #[test]
    fn host_matches_is_port_tolerant_and_rejects_foreign() {
        assert!(host_matches("pad.example.com", "pad.example.com"));
        assert!(host_matches("PAD.example.com", "pad.example.com")); // case-insensitive
        assert!(host_matches("pad.example.com:443", "pad.example.com")); // port-tolerant
        assert!(host_matches("localhost:8080", "localhost:8080"));
        assert!(host_matches("localhost", "localhost:8080")); // port-tolerant
        assert!(!host_matches("attacker.example.com", "pad.example.com"));
        assert!(!host_matches("", "pad.example.com"));
    }
}
