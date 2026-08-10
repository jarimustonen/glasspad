//! The **B2 multi-round** push surface for the hosted share server.
//!
//! `POST /api/v1/pages/{slug}/rounds` — the authoring agent re-renders a live page
//! in response to a submission, turning the one-shot return channel (B1) into a
//! conversational exchange in a single live page. This is the agent-facing write for
//! multi-round; it is the sibling of ingest (`POST /api/v1/pages`) but targets an
//! **existing** page rather than minting a new one:
//!
//! * **API-key authenticated + owner-scoped.** The [`Tenant`] is injected by the same
//!   ingest auth middleware, and the push is refused unless the page's own `meta.json`
//!   records that tenant as owner — a tenant can never re-render another's page (an
//!   opaque `404`, no cross-tenant existence oracle).
//! * **Reuses the frozen boundary.** The new round's body flows through the exact
//!   `render`/wrap seam and is served on the same content route under the same
//!   server-set sandbox/CSP/Trusted-Types headers — pushing a new round widens
//!   nothing (each round stays null-origin, `connect-src 'none'`, no `allow-forms`).
//! * **Immutable baseline preserved.** The re-render is stored as a live overlay, not
//!   an overwrite of the published `artifact.html` (see [`crate::hosted::store`]).
//!
//! On success the connected shell(s) for that page are pushed a keyed `round` event
//! over the live-reload SSE carrier and swap the framed artifact in place. The
//! response carries the new `round` number + `content_version` (the value a
//! submission answering this round must echo — cross-round binding).

use axum::extract::rejection::JsonRejection;
use axum::{
    Extension, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;
use serde_json::json;

use crate::artifact_host::space;
use crate::cli::SCHEMA_VERSION;

use super::HostedState;
use super::auth::{self, KeyTable, Tenant};
use super::ingest::build_artifact_body;
use super::store::RoundError;

/// Build the round-push route. Authenticated (owner re-renders only) and body-limited
/// to a full artifact, exactly like ingest.
pub fn router(state: HostedState, keys: std::sync::Arc<KeyTable>) -> Router {
    let body_limit = space::MAX_FILE_BYTES as usize + 128 * 1024;
    Router::new()
        .route("/api/v1/pages/{slug}/rounds", post(push_round))
        .route_layer(axum::middleware::from_fn_with_state(
            keys,
            auth::ingest_auth,
        ))
        .layer(DefaultBodyLimit::max(body_limit))
        .with_state(state)
}

/// A round-push body: the same `{html | markdown (+ template)}` shape ingest accepts,
/// minus page-creation fields (no slug/tenant/title/idempotency_key — the target page
/// and its owner/title come from the URL + stored meta, never the body).
/// `deny_unknown_fields` so a misspelled field is a `400`, not a silent drop.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoundRequest {
    html: Option<String>,
    markdown: Option<String>,
    template: Option<String>,
}

/// `POST /api/v1/pages/{slug}/rounds`.
async fn push_round(
    State(state): State<HostedState>,
    Extension(tenant): Extension<Tenant>,
    Path(slug): Path<String>,
    body: Result<axum::Json<RoundRequest>, JsonRejection>,
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

    // Owner-scope early (before building the body) so a non-owner learns nothing and
    // does no render work: a missing page and a page owned by another tenant both 404.
    match state.store.page_tenant(&slug) {
        Some(owner) if owner == tenant.0 => {}
        _ => {
            return err(
                StatusCode::NOT_FOUND,
                "no_such_page",
                "no such page for this tenant",
            );
        }
    }

    // Resolve the new round body through the shared ingest seam (exactly one of
    // html/markdown, size-capped, template-rendered).
    let html = match build_artifact_body(
        req.html.as_deref(),
        req.markdown.as_deref(),
        req.template.as_deref(),
    ) {
        Ok(h) => h,
        Err((code, msg)) => return err(StatusCode::BAD_REQUEST, code, &msg),
    };

    let store = state.store.clone();
    let tenant_id = tenant.0.clone();
    let key = slug.clone();
    let result =
        tokio::task::spawn_blocking(move || store.push_round(&tenant_id, &key, html)).await;
    match result {
        Ok(Ok(pushed)) => (
            StatusCode::OK,
            axum::Json(json!({
                "schema_version": SCHEMA_VERSION,
                "slug": slug,
                "round": pushed.round,
                "content_version": pushed.content_version,
                "warnings": [],
            })),
        )
            .into_response(),
        Ok(Err(RoundError::NoSuchPage)) => err(
            StatusCode::NOT_FOUND,
            "no_such_page",
            "no such page for this tenant",
        ),
        Ok(Err(e @ RoundError::Io(_))) => {
            eprintln!("glasspad host: push-round storage error: {e}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not persist the new round",
            )
        }
        Err(join) => {
            eprintln!("glasspad host: push-round task panicked: {join}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not persist the new round",
            )
        }
    }
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        axum::Json(json!({
            "schema_version": SCHEMA_VERSION,
            "error": { "code": code, "message": message },
        })),
    )
        .into_response()
}
