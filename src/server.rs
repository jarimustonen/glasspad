use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use tokio::net::TcpListener;

use crate::routes::{api, render};
use crate::store::PadStore;

pub async fn run(port: u16) {
    let store = Arc::new(PadStore::new(port));

    let app = Router::new()
        .route("/api/pads", post(api::create_pad).get(api::list_pads))
        .route(
            "/api/pads/{id}",
            get(api::get_pad)
                .put(api::update_pad)
                .delete(api::delete_pad),
        )
        .route("/{id}", get(render::get_pad_html))
        .with_state(store);

    let addr = format!("127.0.0.1:{}", port);
    eprintln!("glasspad serving on http://{}", addr);

    let listener = TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
