use std::sync::Arc;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tokio::net::TcpListener;

use crate::artifact_host::{self, guards};
use crate::routes::{api, render};
use crate::store::PadStore;

/// Build the complete, fully-guarded application router. Extracted so tests can
/// exercise the middleware stack (Host / Origin guards) — `artifact_host::router`
/// alone omits them, which would let the security gate pass with the guards
/// absent or misordered.
pub fn build_app(port: u16, store: Arc<PadStore>) -> Router {
    // --- v0.1 control API + legacy pad render (unchanged; removed in Wave 5) ---
    // The control API is guarded per design.md §5: reject `Origin: null` and any
    // foreign origin on control endpoints.
    let control = Router::new()
        .route("/api/pads", post(api::create_pad).get(api::list_pads))
        .route(
            "/api/pads/{id}",
            get(api::get_pad)
                .put(api::update_pad)
                .delete(api::delete_pad),
        )
        .route_layer(middleware::from_fn_with_state(
            port,
            guards::control_origin_guard,
        ))
        .route("/{id}", get(render::get_pad_html))
        .with_state(store);

    // --- v0.2 HTML-artifact host (Wave 1 security gate) ---
    control
        .merge(artifact_host::router(port))
        // Global DNS-rebinding defense: validate the Host header on every route.
        .layer(middleware::from_fn_with_state(port, guards::host_guard))
}

pub async fn run(port: u16) {
    let store = Arc::new(PadStore::new(port));
    let app = build_app(port, store);

    let addr = format!("127.0.0.1:{}", port);
    eprintln!("glasspad serving on http://{}", addr);

    // Loopback-only bind (design.md §5). Binding a routable interface is not
    // offered without an explicit unsafe opt-in (not implemented in Wave 1).
    let listener = TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    fn app() -> Router {
        build_app(3000, Arc::new(PadStore::new(3000)))
    }

    async fn send(req: Request<Body>) -> StatusCode {
        app().oneshot(req).await.unwrap().status()
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

    #[tokio::test]
    async fn control_origin_guard_rejects_null_and_foreign_on_api() {
        // Sandboxed-frame / cross-origin write carries Origin: null → rejected.
        let s = send(
            Request::post("/api/pads")
                .header("host", "127.0.0.1:3000")
                .header("origin", "null")
                .header("content-type", "application/x-yaml")
                .body(Body::from("spec_version: 1\ntitle: x\nsections: []\n"))
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let s = send(
            Request::post("/api/pads")
                .header("host", "127.0.0.1:3000")
                .header("origin", "http://evil.example.com")
                .header("content-type", "application/x-yaml")
                .body(Body::from("spec_version: 1\ntitle: x\nsections: []\n"))
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn control_origin_guard_allows_loopback_origin() {
        // Same-loopback-origin request passes the Origin guard (content-type
        // then drives the rest of the handler; we only assert it's not FORBIDDEN).
        let s = send(
            Request::get("/api/pads")
                .header("host", "127.0.0.1:3000")
                .header("origin", "http://127.0.0.1:3000")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
}
