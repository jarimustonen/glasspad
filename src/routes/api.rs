use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::data::infer::infer_dataset_meta;
use crate::data::types::Dataset;
use crate::models::{Pad, PadCreated, PadMeta};
use crate::security::token as pad_token;
use crate::spec::schema::DashboardSpec;
use crate::spec::validate;
use crate::store::PadStore;

/// Parse datasets from inline_data in sections.
fn collect_inline_datasets(spec: &DashboardSpec) -> BTreeMap<String, Dataset> {
    let mut datasets = BTreeMap::new();
    for section in &spec.sections {
        if let Some(ref data) = section.inline_data {
            // Use section id or title as synthetic dataset name
            let name = section
                .source
                .clone()
                .or_else(|| section.id.clone())
                .unwrap_or_else(|| format!("_inline_{}", section.title));
            datasets.insert(name, data.clone());
        }
    }
    datasets
}

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

    // Only accept YAML specs now (no raw HTML fallback)
    if !content_type.contains("yaml")
        && !body_str.trim_start().starts_with("spec_version:")
        && !body_str.trim_start().starts_with("title:")
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Expected YAML spec (Content-Type: application/x-yaml)".to_string(),
        ));
    }

    let spec: DashboardSpec = serde_yaml::from_str(&body_str)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e)))?;

    // Collect inline datasets
    let datasets = collect_inline_datasets(&spec);
    let provided: HashSet<String> = datasets.keys().cloned().collect();

    // Validate
    let errors = validate::validate(&spec, &provided);
    if !errors.is_empty() {
        let msg = errors
            .iter()
            .map(|e| format!("  - {}", e))
            .collect::<Vec<_>>()
            .join("\n");
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Spec validation failed:\n{}", msg),
        ));
    }

    // Generate metadata
    let dataset_meta = datasets
        .iter()
        .map(|(name, data)| (name.clone(), infer_dataset_meta(data)))
        .collect();

    let id = Uuid::new_v4().simple().to_string();
    let token = pad_token::generate_token();
    let now = Utc::now();
    let title = spec.title.clone();

    let pad = Pad {
        id: id.clone(),
        token: token.clone(),
        title: title.clone(),
        spec,
        datasets,
        dataset_meta,
        created_at: now,
    };

    let url = format!("{}/{}", store.base_url, id);
    store.insert(pad);

    Ok((
        StatusCode::CREATED,
        Json(PadCreated {
            id,
            url,
            title,
            token,
            created_at: now,
        }),
    ))
}

pub async fn list_pads(State(store): State<Arc<PadStore>>) -> Json<Vec<PadMeta>> {
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
            pad_type: "dashboard".to_string(),
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

    // Verify token
    let provided_token = headers
        .get("x-glasspad-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !pad_token::verify_token(provided_token, &existing.token) {
        return Err((StatusCode::FORBIDDEN, "Invalid token".to_string()));
    }

    let body_str = String::from_utf8(body.to_vec())
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid UTF-8".to_string()))?;

    let spec: DashboardSpec = serde_yaml::from_str(&body_str)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e)))?;

    let datasets = collect_inline_datasets(&spec);
    let provided: HashSet<String> = datasets.keys().cloned().collect();

    let errors = validate::validate(&spec, &provided);
    if !errors.is_empty() {
        let msg = errors.iter().map(|e| format!("  - {}", e)).collect::<Vec<_>>().join("\n");
        return Err((StatusCode::BAD_REQUEST, format!("Spec validation failed:\n{}", msg)));
    }

    let dataset_meta = datasets
        .iter()
        .map(|(name, data)| (name.clone(), infer_dataset_meta(data)))
        .collect();

    let pad = Pad {
        id: id.clone(),
        token: existing.token,
        title: spec.title.clone(),
        spec,
        datasets,
        dataset_meta,
        created_at: existing.created_at,
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
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, String)> {
    let existing = store
        .get(&id)
        .ok_or((StatusCode::NOT_FOUND, "Pad not found".to_string()))?;

    let provided_token = headers
        .get("x-glasspad-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !pad_token::verify_token(provided_token, &existing.token) {
        return Err((StatusCode::FORBIDDEN, "Invalid token".to_string()));
    }

    if store.delete(&id) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "Pad not found".to_string()))
    }
}
