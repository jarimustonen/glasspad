use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};

use crate::renderer;
use crate::security::csp;
use crate::store::PadStore;

pub async fn get_pad_html(
    State(store): State<Arc<PadStore>>,
    Path(id): Path<String>,
) -> Result<Response, StatusCode> {
    let pad = store.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    let html = renderer::render_dashboard(&pad);

    Ok((
        [(header::CONTENT_SECURITY_POLICY, csp::CSP_HEADER_VALUE)],
        Html(html),
    ).into_response())
}
