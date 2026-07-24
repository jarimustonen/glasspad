//! v0.2 HTML-artifact host — **Wave 1 security gate**.
//!
//! Stands up the null-origin sandboxed iframe host and its security contract,
//! *alongside* the existing v0.1 pad server (no old code is removed this wave —
//! that is Wave 5). See `issues/html-artifact-host-rewrite/{design,plan,wave-plan}.md`.
//!
//! URL topology (design.md §/plan.md §3):
//! * `GET /{space}/`            — space entry (trusted shell for the home artifact)
//! * `GET /{space}/{slug}`      — trusted shell framing the artifact
//! * `GET /{space}/_c/{slug}`   — raw artifact document (carries the sandbox CSP)
//! * `GET /_gp/v1/*`            — pinned base libraries (Wave 1 ships stubs)

pub mod fixtures;
pub mod guards;
pub mod headers;
pub mod shell;

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;

use crate::security::token;

/// Shared state for the artifact host. Only the port is needed in Wave 1 — the
/// CSP must name the explicit host, and fixtures are static. Wave 2a replaces the
/// static fixture registry with a live directory snapshot.
pub struct ArtifactHost {
    pub port: u16,
}

impl ArtifactHost {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

/// Build the artifact-host router, finalized to `Router<()>` so it can be merged
/// into the main server router. The control-plane guards (§5) are applied by the
/// caller in `server::run` across the whole app.
pub fn router(port: u16) -> Router {
    let state = Arc::new(ArtifactHost::new(port));
    Router::new()
        .route("/{space}/", get(space_entry))
        .route("/{space}/_c/{slug}", get(artifact_content))
        .route("/{space}/{slug}", get(shell_page))
        .route("/_gp/v1/{*path}", get(gp_asset))
        .with_state(state)
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

const RESERVED: &[&str] = &["_gp", "_c", "assets", "api"];

fn valid_space(s: &str) -> bool {
    valid_name(s) && !RESERVED.contains(&s)
}

// --- Handlers -------------------------------------------------------------

#[derive(Deserialize, Default)]
struct ContentQuery {
    /// Diagnostic knob for the adversarial suite. `csp=noeval` serves the CSP
    /// **without** `'unsafe-eval'` — strictly tighter, never looser — so the
    /// suite can prove Vega-Lite's `new Function` truly needs it (design.md §4).
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
    let Some(fixture) = fixtures::get(&space, &slug) else {
        return not_found();
    };
    let allow_eval = q.csp.as_deref() != Some("noeval");
    let csp = headers::artifact_csp(host.port, allow_eval);

    let mut hmap = base_html_headers();
    hmap.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp).expect("csp is header-safe ascii"),
    );
    for (name, value) in headers::hardening_headers() {
        hmap.insert(name, value);
    }
    (hmap, axum::response::Html(fixture.html)).into_response()
}

/// Trusted shell framing the artifact.
async fn shell_page(
    State(host): State<Arc<ArtifactHost>>,
    Path((space, slug)): Path<(String, String)>,
) -> Response {
    if !valid_space(&space) || !valid_name(&slug) {
        return not_found();
    }
    if fixtures::get(&space, &slug).is_none() {
        return not_found();
    }
    render_shell(&host, &space, &slug)
}

/// Space entry — the shell for the home artifact (`index`, else first slug).
async fn space_entry(
    State(host): State<Arc<ArtifactHost>>,
    Path(space): Path<String>,
) -> Response {
    if !valid_space(&space) {
        return not_found();
    }
    let slugs = fixtures::slugs(&space);
    if slugs.is_empty() {
        return not_found();
    }
    let home = if slugs.contains(&"index") { "index" } else { slugs[0] };
    render_shell(&host, &space, home)
}

fn render_shell(host: &ArtifactHost, space: &str, slug: &str) -> Response {
    let nonce = token::generate_token();
    let known = fixtures::slugs(space);
    let body = shell::render(space, slug, &known, &nonce);
    let csp = headers::shell_csp(&nonce);

    let mut hmap = base_html_headers();
    hmap.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp).expect("csp is header-safe ascii"),
    );
    // The shell must never be framed by anyone.
    hmap.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    );
    for (name, value) in headers::hardening_headers() {
        hmap.insert(name, value);
    }
    let _ = host; // port already baked into the framed content path
    (hmap, axum::response::Html(body)).into_response()
}

/// `/_gp/v1/*` pinned base libraries. Wave 1 ships stubs; classic `<script src>`
/// / `<link>` don't need CORS, but the requests that are CORS-gated get
/// `Access-Control-Allow-Origin: *` (no credentials) per design.md §4.
async fn gp_asset(Path(path): Path<String>) -> Response {
    // Path-traversal guard: the wildcard is a single flat filename here.
    if path.contains("..") || path.contains('/') {
        return not_found();
    }
    let Some((content_type, body)) = fixtures::gp_asset(&path) else {
        return not_found();
    };
    let mut hmap = HeaderMap::new();
    hmap.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type),
    );
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
    h
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // --- HTTP-level contract tests (deterministic gate; the browser suite
    //     proves the browser actually *enforces* these headers). ---

    use axum::body::Body;
    use axum::http::Request;
    use tower::util::ServiceExt;

    async fn get(path: &str) -> axum::http::Response<Body> {
        router(3000)
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    fn header<'a>(resp: &'a axum::http::Response<Body>, name: &str) -> &'a str {
        resp.headers().get(name).map(|v| v.to_str().unwrap()).unwrap_or("")
    }

    #[tokio::test]
    async fn content_route_carries_full_security_contract() {
        let resp = get("/demo/_c/index").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let csp = header(&resp, "content-security-policy");
        assert!(csp.starts_with("sandbox allow-scripts"), "sandbox missing: {csp}");
        assert!(csp.contains("connect-src 'none'"), "egress open: {csp}");
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains(r#"sandbox="allow-scripts allow-top-navigation-by-user-activation""#));
        assert!(!html.contains("allow-same-origin"));
    }

    #[tokio::test]
    async fn space_entry_serves_home() {
        let resp = get("/demo/").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("/demo/_c/index"));
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
}
