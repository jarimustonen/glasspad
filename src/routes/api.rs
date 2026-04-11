use std::collections::{BTreeMap, BTreeSet};
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

/// Require a YAML Content-Type header. Rejects missing, malformed, or non-YAML types.
fn require_yaml_content_type(headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    let raw = headers
        .get("content-type")
        .ok_or((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected Content-Type: application/x-yaml".to_string(),
        ))?
        .to_str()
        .map_err(|_| (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Invalid Content-Type header".to_string(),
        ))?;

    let media_type = raw.split(';').next().unwrap_or("").trim();
    if !matches!(media_type, "application/x-yaml" | "application/yaml" | "text/yaml") {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected Content-Type: application/x-yaml".to_string(),
        ));
    }
    Ok(())
}

/// Collect datasets from both top-level spec.datasets (external, provided via --data)
/// and section.inline_data (inline in spec). Detects duplicate/conflicting definitions.
fn collect_datasets(
    spec: &DashboardSpec,
    external_datasets: &BTreeMap<String, Dataset>,
) -> Result<BTreeMap<String, Dataset>, String> {
    let mut datasets = BTreeMap::new();

    // 1. External datasets provided via --data (already parsed by CLI or multipart)
    for (name, data) in external_datasets {
        datasets.insert(name.clone(), data.clone());
    }

    // 2. Inline datasets from sections
    for (idx, section) in spec.sections.iter().enumerate() {
        if let Some(ref data) = section.inline_data {
            let name = section
                .inline_dataset_name(idx)
                .expect("inline_data is Some");

            if let Some(existing) = datasets.get(&name) {
                // Reject if an external dataset collides with inline data (even if
                // payloads match — origin ambiguity hides stale --data flags).
                if external_datasets.contains_key(&name) {
                    return Err(format!(
                        "Dataset '{}' is provided both externally and via inline_data (section [{}] \"{}\")",
                        name, idx, section.title
                    ));
                }
                if existing != data {
                    return Err(format!(
                        "Conflicting dataset definitions for '{}' (section [{}] \"{}\")",
                        name, idx, section.title
                    ));
                }
                // Same inline data, already present — skip
            } else {
                datasets.insert(name, data.clone());
            }
        }
    }

    Ok(datasets)
}

pub async fn create_pad(
    State(store): State<Arc<PadStore>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<PadCreated>), (StatusCode, String)> {
    require_yaml_content_type(&headers)?;

    let spec: DashboardSpec = serde_yaml::from_slice(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e)))?;

    // Collect datasets (no external datasets via API body-only path)
    let external = BTreeMap::new();
    let datasets = collect_datasets(&spec, &external)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let external_names: BTreeSet<String> = external.keys().cloned().collect();

    let errors = validate::validate(&spec, &external_names);
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
    let pad = store.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(PadMeta {
        id: pad.id.clone(),
        title: pad.title.clone(),
        pad_type: "dashboard".to_string(),
        url: format!("{}/{}", store.base_url, pad.id),
        created_at: pad.created_at,
    }))
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

    let provided_token = headers
        .get("x-glasspad-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !pad_token::verify_token(provided_token, &existing.token) {
        return Err((StatusCode::FORBIDDEN, "Invalid token".to_string()));
    }

    require_yaml_content_type(&headers)?;

    let spec: DashboardSpec = serde_yaml::from_slice(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e)))?;

    let external = BTreeMap::new();
    let datasets = collect_datasets(&spec, &external)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let external_names: BTreeSet<String> = external.keys().cloned().collect();

    let errors = validate::validate(&spec, &external_names);
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
        token: existing.token.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    // --- Unit tests for require_yaml_content_type ---

    fn headers_with(ct: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("content-type", HeaderValue::from_str(ct).unwrap());
        h
    }

    #[test]
    fn rejects_missing_content_type() {
        let h = HeaderMap::new();
        let err = require_yaml_content_type(&h).unwrap_err();
        assert_eq!(err.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn rejects_malformed_content_type() {
        let mut h = HeaderMap::new();
        h.insert("content-type", HeaderValue::from_bytes(b"application/\xff").unwrap());
        let err = require_yaml_content_type(&h).unwrap_err();
        assert_eq!(err.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn rejects_json_content_type() {
        let err = require_yaml_content_type(&headers_with("application/json")).unwrap_err();
        assert_eq!(err.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn rejects_substring_trick() {
        // "yaml" appears as substring but is not a valid YAML media type
        let err = require_yaml_content_type(&headers_with("application/notyaml")).unwrap_err();
        assert_eq!(err.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn rejects_yaml_in_parameter() {
        let err = require_yaml_content_type(&headers_with("application/json; note=yaml")).unwrap_err();
        assert_eq!(err.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn accepts_application_x_yaml() {
        require_yaml_content_type(&headers_with("application/x-yaml")).unwrap();
    }

    #[test]
    fn accepts_application_yaml() {
        require_yaml_content_type(&headers_with("application/yaml")).unwrap();
    }

    #[test]
    fn accepts_text_yaml() {
        require_yaml_content_type(&headers_with("text/yaml")).unwrap();
    }

    #[test]
    fn accepts_yaml_with_charset() {
        require_yaml_content_type(&headers_with("application/x-yaml; charset=utf-8")).unwrap();
    }

    // --- Integration tests for endpoint wiring ---

    use axum::{body::Body, http::Request, routing::{post, get}, Router};
    use tower::util::ServiceExt;

    const MINIMAL_SPEC: &str = "spec_version: 1\ntitle: test\nsections:\n  - title: s1\n    type: markdown\n    markdown:\n      content: hello\n";

    fn app() -> Router {
        let store = Arc::new(PadStore::new(9999));
        Router::new()
            .route("/api/pads", post(create_pad))
            .route("/api/pads/{id}", get(get_pad).put(update_pad))
            .with_state(store)
    }

    #[tokio::test]
    async fn create_rejects_non_yaml_content_type() {
        let resp = app()
            .oneshot(
                Request::post("/api/pads")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn update_rejects_non_yaml_content_type() {
        let app = app();

        // Create a pad first
        let create_resp = app
            .clone()
            .oneshot(
                Request::post("/api/pads")
                    .header("content-type", "application/x-yaml")
                    .body(Body::from(MINIMAL_SPEC))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_resp.status(), StatusCode::CREATED);
        let body_bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let id = created["id"].as_str().unwrap();
        let token = created["token"].as_str().unwrap();

        // Try to update with wrong content type
        let resp = app
            .oneshot(
                Request::put(format!("/api/pads/{}", id))
                    .header("content-type", "application/json")
                    .header("x-glasspad-token", token)
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}
