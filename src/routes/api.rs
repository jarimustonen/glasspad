use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    body::Bytes,
};
use chrono::Utc;
use uuid::Uuid;

use crate::models::{DashboardSpec, Pad, PadContent, PadCreated, PadMeta};
use crate::store::PadStore;

pub async fn create_pad(
    State(store): State<Arc<PadStore>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<PadCreated>), (StatusCode, String)> {
    let body_str = String::from_utf8(body.to_vec())
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid UTF-8".to_string()))?;

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let (title, pad_type, content, raw_yaml) =
        if content_type.contains("yaml") || body_str.trim_start().starts_with("title:") {
            // Parse as YAML dashboard spec
            let spec: DashboardSpec = serde_yaml::from_str(&body_str)
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e)))?;
            let title = spec.title.clone();
            (
                title,
                "dashboard".to_string(),
                PadContent::Dashboard(spec),
                Some(body_str),
            )
        } else if content_type.contains("json") {
            // Parse as JSON with type/title/content fields
            let val: serde_json::Value = serde_json::from_str(&body_str)
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)))?;
            let title = val["title"]
                .as_str()
                .unwrap_or("Untitled")
                .to_string();
            let html = val["content"]
                .as_str()
                .unwrap_or("")
                .to_string();
            (title, "html".to_string(), PadContent::RawHtml(html), None)
        } else {
            // Try YAML first, fall back to raw HTML
            match serde_yaml::from_str::<DashboardSpec>(&body_str) {
                Ok(spec) => {
                    let title = spec.title.clone();
                    (
                        title,
                        "dashboard".to_string(),
                        PadContent::Dashboard(spec),
                        Some(body_str),
                    )
                }
                Err(_) => (
                    "Untitled".to_string(),
                    "html".to_string(),
                    PadContent::RawHtml(body_str),
                    None,
                ),
            }
        };

    let id = Uuid::new_v4().to_string()[..8].to_string();
    let now = Utc::now();

    let pad = Pad {
        id: id.clone(),
        title: title.clone(),
        pad_type,
        content,
        created_at: now,
        raw_yaml,
    };

    let url = format!("{}/{}", store.base_url, id);
    store.insert(pad);

    Ok((
        StatusCode::CREATED,
        Json(PadCreated {
            id,
            url,
            title,
            created_at: now,
        }),
    ))
}

pub async fn list_pads(
    State(store): State<Arc<PadStore>>,
) -> Json<Vec<PadMeta>> {
    Json(store.list())
}

pub async fn get_pad(
    State(store): State<Arc<PadStore>>,
    Path(id): Path<String>,
) -> Result<Json<PadMeta>, StatusCode> {
    match store.get(&id) {
        Some(pad) => Ok(Json(PadMeta {
            id: pad.id.clone(),
            title: pad.title,
            pad_type: pad.pad_type,
            url: format!("{}/{}", store.base_url, pad.id),
            created_at: pad.created_at,
        })),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn update_pad(
    State(store): State<Arc<PadStore>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    let existing = store
        .get(&id)
        .ok_or((StatusCode::NOT_FOUND, "Pad not found".to_string()))?;

    let body_str = String::from_utf8(body.to_vec())
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid UTF-8".to_string()))?;

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let (title, pad_type, content, raw_yaml) =
        if content_type.contains("yaml") || body_str.trim_start().starts_with("title:") {
            let spec: DashboardSpec = serde_yaml::from_str(&body_str)
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e)))?;
            let title = spec.title.clone();
            (
                title,
                "dashboard".to_string(),
                PadContent::Dashboard(spec),
                Some(body_str),
            )
        } else {
            (
                existing.title.clone(),
                "html".to_string(),
                PadContent::RawHtml(body_str),
                None,
            )
        };

    let pad = Pad {
        id: id.clone(),
        title,
        pad_type,
        content,
        created_at: existing.created_at,
        raw_yaml,
    };

    if store.update(&id, pad) {
        Ok(StatusCode::OK)
    } else {
        Err((StatusCode::NOT_FOUND, "Pad not found".to_string()))
    }
}

pub async fn delete_pad(
    State(store): State<Arc<PadStore>>,
    Path(id): Path<String>,
) -> StatusCode {
    if store.delete(&id) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
