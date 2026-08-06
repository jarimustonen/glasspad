//! The ingest handler: `POST /api/v1/pages` (auth required).
//!
//! Accepts one page as JSON and publishes it as an immutable, sandboxed artifact.
//! The body is untrusted, exactly like a locally `create`d/`render`ed body — it
//! governs only the artifact **body**; the sandbox/CSP/Trusted-Types boundary is
//! set server-side on the read response and cannot be widened from here (plan §7).
//!
//! Two shapes (exactly one required):
//! * `{"html": "<…>"}` — a full document or fragment, served as-is via the shared
//!   wrap/content seam.
//! * `{"markdown": "…", "template": "prose"|"dashboard"|"<inline template html>"}`
//!   — rendered server-side through the shared `render` path (a bare built-in name
//!   selects a built-in; any other `template` string is an inline template that
//!   must carry one `{{content}}`).
//!
//! Optional `"title"` overrides the resolved display title.

use axum::extract::rejection::JsonRejection;
use axum::{
    Extension,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

use crate::artifact_host::render;
use crate::artifact_host::space;
use crate::cli::SCHEMA_VERSION;
use crate::server::enforce_body_cap;

use super::HostedState;
use super::auth::Tenant;
use super::store::PublishError;

/// The ingest request body.
#[derive(Deserialize)]
pub struct PublishRequest {
    html: Option<String>,
    markdown: Option<String>,
    template: Option<String>,
    title: Option<String>,
}

/// `POST /api/v1/pages`. The [`Tenant`] is injected by the auth middleware (never
/// read from the body). Reuses the exact `render`/wrap seam; on success returns
/// `201` with `{slug, url, title}`.
pub async fn publish(
    State(state): State<HostedState>,
    Extension(tenant): Extension<Tenant>,
    body: Result<axum::Json<PublishRequest>, JsonRejection>,
) -> Response {
    let axum::Json(req) = match body {
        Ok(b) => b,
        Err(rej) => {
            return err(
                StatusCode::BAD_REQUEST,
                "invalid_json",
                &format!("request body is not valid JSON for this endpoint: {rej}"),
            );
        }
    };

    // Bound the title override (it lands in metadata + the trusted shell); an
    // unbounded title is a metadata-DoS vector. The shell escapes it regardless,
    // so this is a size bound, not the injection defense.
    if let Some(t) = &req.title
        && t.chars().count() > space::MAX_TITLE_CHARS
    {
        return err(
            StatusCode::BAD_REQUEST,
            "title_too_long",
            &format!("title exceeds {} characters", space::MAX_TITLE_CHARS),
        );
    }

    // Resolve the artifact body from exactly one of html / markdown.
    let html = match resolve_body(&req) {
        Ok(h) => h,
        Err((code, msg)) => return err(StatusCode::BAD_REQUEST, code, &msg),
    };

    // The publish does blocking filesystem I/O and holds the store mutation lock —
    // run it off the async worker so it never blocks the runtime.
    let store = state.store.clone();
    let tenant_id = tenant.0.clone();
    let title = req.title.clone();
    let result = tokio::task::spawn_blocking(move || store.publish(&tenant_id, html, title)).await;
    let result = match result {
        Ok(r) => r,
        Err(join) => {
            eprintln!("glasspad host: publish task panicked: {join}");
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not persist the page",
            );
        }
    };

    match result {
        Ok(published) => {
            let url = format!("{}{}/{}/", state.public_origin, state.mount, published.slug);
            let payload = json!({
                "schema_version": SCHEMA_VERSION,
                "slug": published.slug,
                "url": url,
                "title": published.title,
                "warnings": [],
            });
            (StatusCode::CREATED, axum::Json(payload)).into_response()
        }
        Err(PublishError::Full) => err(
            StatusCode::INSUFFICIENT_STORAGE,
            "store_full",
            &PublishError::Full.to_string(),
        ),
        Err(e @ PublishError::SlugExhausted) => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "slug_exhausted",
            &e.to_string(),
        ),
        Err(e @ PublishError::Io(_)) => {
            eprintln!("glasspad host: ingest storage error: {e}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not persist the page",
            )
        }
    }
}

/// Resolve the artifact HTML body from the request, enforcing "exactly one of
/// html/markdown" and the input size caps. For markdown, renders through the shared
/// template path and bounds the generated body to the per-artifact cap.
fn resolve_body(req: &PublishRequest) -> Result<String, (&'static str, String)> {
    let has_html = req.html.as_ref().is_some_and(|h| !h.trim().is_empty());
    let has_md = req.markdown.as_ref().is_some_and(|m| !m.trim().is_empty());

    match (has_html, has_md) {
        (true, true) => Err((
            "conflicting_body",
            "provide exactly one of `html` or `markdown`, not both".to_string(),
        )),
        (false, false) => Err((
            "missing_body",
            "provide either `html` (a document/fragment) or `markdown` (+ optional `template`)"
                .to_string(),
        )),
        (true, false) => {
            let html = req.html.clone().unwrap();
            if html.len() as u64 > space::MAX_FILE_BYTES {
                return Err((
                    "html_too_large",
                    format!(
                        "html is {} bytes, over the {}-byte per-artifact limit",
                        html.len(),
                        space::MAX_FILE_BYTES
                    ),
                ));
            }
            Ok(html)
        }
        (false, true) => {
            let md = req.markdown.clone().unwrap();
            if md.len() as u64 > space::MAX_FILE_BYTES {
                return Err((
                    "markdown_too_large",
                    format!(
                        "markdown is {} bytes, over the {}-byte per-artifact limit",
                        md.len(),
                        space::MAX_FILE_BYTES
                    ),
                ));
            }
            let template = resolve_template(req.template.as_deref())?;
            let body = render::render_to_body(&md, &template)
                .map_err(|e| ("invalid_template", e.to_string()))?;
            enforce_body_cap(body).map_err(|m| ("rendered_output_too_large", m))
        }
    }
}

/// Resolve the `template` field to a template string: a bare built-in name selects
/// a built-in; any other value is treated as an inline template (bounded to the
/// per-file cap). Absent → the default built-in (`prose`).
fn resolve_template(template: Option<&str>) -> Result<String, (&'static str, String)> {
    match template {
        None => Ok(render::builtin_template(render::DEFAULT_TEMPLATE)
            .expect("default template is a built-in")
            .to_string()),
        Some(reference) => {
            if let Some(builtin) = render::builtin_template(reference) {
                return Ok(builtin.to_string());
            }
            if reference.len() as u64 > space::MAX_FILE_BYTES {
                return Err((
                    "template_too_large",
                    format!(
                        "inline template is {} bytes, over the {}-byte limit",
                        reference.len(),
                        space::MAX_FILE_BYTES
                    ),
                ));
            }
            // An inline template — validated for the single `{{content}}` slot by
            // `render_to_body` at render time.
            Ok(reference.to_string())
        }
    }
}

/// A JSON error envelope on the ingest surface (AI-first §10: structured, with a
/// stable `code`). Body detail never includes auth internals.
fn err(status: StatusCode, code: &str, message: &str) -> Response {
    let payload = json!({
        "schema_version": SCHEMA_VERSION,
        "error": { "code": code, "message": message },
    });
    (status, axum::Json(payload)).into_response()
}
