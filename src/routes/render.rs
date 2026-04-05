use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Html,
};

use crate::models::PadContent;
use crate::renderer;
use crate::store::PadStore;

pub async fn get_pad_html(
    State(store): State<Arc<PadStore>>,
    Path(id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let pad = store.get(&id).ok_or(StatusCode::NOT_FOUND)?;

    let html = match &pad.content {
        PadContent::Dashboard(spec) => renderer::render_dashboard(spec),
        PadContent::RawHtml(html) => html.clone(),
    };

    Ok(Html(html))
}
