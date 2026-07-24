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

pub async fn run(port: u16) {
    let store = Arc::new(PadStore::new(port));

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
    let app = control
        .merge(artifact_host::router(port))
        // Global DNS-rebinding defense: validate the Host header on every route.
        .layer(middleware::from_fn_with_state(port, guards::host_guard));

    let addr = format!("127.0.0.1:{}", port);
    eprintln!("glasspad serving on http://{}", addr);

    // Loopback-only bind (design.md §5). Binding a routable interface is not
    // offered without an explicit unsafe opt-in (not implemented in Wave 1).
    let listener = TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
