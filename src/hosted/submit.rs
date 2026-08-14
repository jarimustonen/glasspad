//! Return-channel HTTP surface for the **hosted** share server.
//!
//! Three endpoints implement the agent↔human round-trip for a hosted page
//! (`issues/artifact-return-channel/{design,models-comparison}.md`):
//!
//! * `POST /api/v1/pages/{slug}/submit` — the **write**, called by the trusted
//!   shell (a browser), **not** the artifact. It is *public by design* — the
//!   unguessable capability slug is the capability, exactly as page read is — so
//!   it carries no API key (the shell can't; it is served to every visitor). CSRF
//!   is closed by an `Origin` allowlist, and floods by a size cap + per-page rate
//!   limit. The submission's `slug`, owning `tenant`, and `content_version` are
//!   all bound **server-side** from the trusted request context (the URL path and
//!   the page's own stored meta/body) — never from the artifact-supplied payload.
//! * `GET /api/v1/pages/{slug}/submissions?since=<cursor>` — the **plain poll**
//!   (A1 fallback). API-key authenticated and per-tenant scoped: a tenant may read
//!   a page's submissions only when it owns that page.
//! * `GET /api/v1/pages/{slug}/submissions/wait?since=<cursor>&timeout=<secs>` —
//!   the **server-side long-poll** (A3 primary). Same auth/scoping, holds the
//!   connection until a submission after the cursor lands or the timeout fires.
//! * `GET /api/v1/pages/{slug}/submissions/stream?since=<cursor>` — the **server-
//!   push SSE transport** (A2). Same auth/scoping; holds an `EventSource` and pushes
//!   each submission after the cursor as a `submission` event (its id stamped as the
//!   SSE `id`, so a `Last-Event-ID` reconnect resumes from the cursor). For an agent
//!   watching many pages or wanting sub-second streaming; the long-poll stays primary.
//!
//! The read endpoints are gated by [`auth::ingest_auth`]; the submit endpoint is
//! not, so it is registered on a separate sub-router without that layer.

use axum::extract::rejection::JsonRejection;
use axum::{
    Extension, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;

use crate::cli::SCHEMA_VERSION;
use crate::submissions::{
    self, DEFAULT_WAIT_SECS, MAX_LIST, MAX_SUBMISSION_BYTES, MAX_WAIT_SECS, SubmitError,
    WaitOutcome,
};

use super::HostedState;
use super::auth::{self, Tenant};

/// The single artifact slug a hosted page holds (its `index`).
const HOSTED_ARTIFACT: &str = "index";

/// Build the return-channel routes. The submit route is public (shell-callable);
/// the read routes carry the ingest API-key auth layer. Both live under
/// `/api/v1/pages/{slug}/…` at the origin root (the shell's `connect-src 'self'`
/// permits the same-origin POST/GET).
pub fn router(state: HostedState, keys: std::sync::Arc<auth::KeyTable>) -> Router {
    // Public write: no API key (the capability slug is the capability). A tight
    // body limit — a submission is a form answer, not a file — plus CSRF/flood
    // defenses inside the handler.
    let submit_body_limit = MAX_SUBMISSION_BYTES + 16 * 1024;
    let submit = Router::new()
        .route("/api/v1/pages/{slug}/submit", post(submit))
        .layer(DefaultBodyLimit::max(submit_body_limit))
        .with_state(state.clone());

    // Authenticated + per-tenant-scoped reads.
    let read = Router::new()
        .route("/api/v1/pages/{slug}/submissions", get(list))
        .route("/api/v1/pages/{slug}/submissions/wait", get(wait))
        .route("/api/v1/pages/{slug}/submissions/stream", get(stream))
        .route_layer(axum::middleware::from_fn_with_state(
            keys,
            auth::ingest_auth,
        ))
        .with_state(state);

    submit.merge(read)
}

/// Submit request body from the trusted shell. `data` is the untrusted user
/// payload (size-bounded, stored opaque). `content_version` is the artifact's
/// self-reported version echo (validated against the server's authoritative value
/// — cross-round protection). `slug` is the shell's currently-viewed artifact slug
/// **within** the space: the space-level submit endpoint binds the space from the
/// URL, but `slug` disambiguates *which page* of a multi-page space the form was on,
/// so the authoritative `content_version` is computed from the right body (a foreign/
/// unknown value falls back safely — see [`Store::page_body`]). Never used for
/// ownership, which is always read from the page's own meta.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitRequest {
    data: serde_json::Value,
    #[serde(default)]
    content_version: Option<String>,
    #[serde(default)]
    slug: Option<String>,
}

/// `POST /api/v1/pages/{slug}/submit`.
async fn submit(
    State(state): State<HostedState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    body: Result<axum::Json<SubmitRequest>, JsonRejection>,
) -> Response {
    // CSRF: the only legitimate caller is the trusted shell, whose `fetch` POST
    // always carries an `Origin` naming our own public origin. Fail-closed — a
    // request with a foreign `Origin` OR none at all (curl/server-to-server) is
    // rejected (see `origin_ok`).
    if !origin_ok(&headers, std::slice::from_ref(&state.public_origin)) {
        return err(
            StatusCode::FORBIDDEN,
            "bad_origin",
            "cross-origin submit rejected",
        );
    }

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

    // The space is bound from the URL path; the page must be served and its owner is
    // read from its own meta (never the payload). `req.slug` picks the viewed page
    // within a multi-page space so the content-version is checked against the right
    // body; it is validated inside `page_body` and never affects ownership.
    let Some(body_html) = state.store.page_body(&slug, req.slug.as_deref()) else {
        return err(StatusCode::NOT_FOUND, "no_such_page", "no such page");
    };
    let Some(tenant) = state.store.page_tenant(&slug) else {
        return err(StatusCode::NOT_FOUND, "no_such_page", "no such page");
    };
    let server_version = submissions::content_version(&body_html);

    // Cross-round protection: if the artifact echoed a content-version, it must
    // match the page's current authoritative version, else the submission is for a
    // round that no longer matches what is served.
    if let Some(echo) = req.content_version.as_deref()
        && echo != server_version
    {
        return err(
            StatusCode::CONFLICT,
            "content_version_mismatch",
            "the submission answers a stale version of this page",
        );
    }

    let store = state.submissions.clone();
    let result = tokio::task::spawn_blocking(move || {
        store.submit(&slug, HOSTED_ARTIFACT, &tenant, &server_version, req.data)
    })
    .await;
    match result {
        Ok(Ok(sub)) => (
            StatusCode::CREATED,
            axum::Json(json!({
                "schema_version": SCHEMA_VERSION,
                "id": sub.id,
                "content_version": sub.content_version,
            })),
        )
            .into_response(),
        Ok(Err(e)) => submit_error(e),
        Err(join) => {
            eprintln!("glasspad host: submit task panicked: {join}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not persist the submission",
            )
        }
    }
}

#[derive(Deserialize)]
struct ListQuery {
    since: Option<u64>,
}

/// `GET /api/v1/pages/{slug}/submissions?since=<cursor>` — plain poll.
async fn list(
    State(state): State<HostedState>,
    Extension(tenant): Extension<Tenant>,
    Path(slug): Path<String>,
    Query(q): Query<ListQuery>,
) -> Response {
    if let Some(resp) = authorize_read(&state, &tenant, &slug) {
        return resp;
    }
    let since = q.since.unwrap_or(0);
    let store = state.submissions.clone();
    let key = slug.clone();
    match tokio::task::spawn_blocking(move || store.list_since(&key, since, MAX_LIST)).await {
        Ok(Ok(page)) => list_response(&page, false),
        Ok(Err(e)) => {
            eprintln!("glasspad host: list submissions error: {e}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not read submissions",
            )
        }
        Err(join) => {
            eprintln!("glasspad host: list task panicked: {join}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not read submissions",
            )
        }
    }
}

#[derive(Deserialize)]
struct WaitQuery {
    since: Option<u64>,
    timeout: Option<u64>,
}

/// `GET /api/v1/pages/{slug}/submissions/wait?since=<cursor>&timeout=<secs>` —
/// server-side long-poll.
async fn wait(
    State(state): State<HostedState>,
    Extension(tenant): Extension<Tenant>,
    Path(slug): Path<String>,
    Query(q): Query<WaitQuery>,
) -> Response {
    if let Some(resp) = authorize_read(&state, &tenant, &slug) {
        return resp;
    }
    let since = q.since.unwrap_or(0);
    let timeout = clamp_timeout(q.timeout);
    match submissions::wait(state.submissions.clone(), slug, since, timeout, MAX_LIST).await {
        Ok(WaitOutcome::Ready(page)) => list_response(&page, false),
        Ok(WaitOutcome::TimedOut { cursor }) => list_response(
            &submissions::ListPage {
                submissions: Vec::new(),
                cursor,
            },
            true,
        ),
        Ok(WaitOutcome::TooBusy) => err(
            StatusCode::SERVICE_UNAVAILABLE,
            "too_many_waiters",
            "the server is holding too many long-polls; retry with the plain poll",
        ),
        Err(e) => {
            eprintln!("glasspad host: wait error: {e}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not read submissions",
            )
        }
    }
}

/// `GET /api/v1/pages/{slug}/submissions/stream?since=<cursor>` — server-push SSE.
/// Same API-key auth + per-tenant scoping as the poll/wait reads (a tenant may stream
/// a page only when it owns it — a cross-tenant or unknown page is an opaque 404,
/// decided **before** any stream is opened, so no submission bytes can leak). The
/// cursor is the `since` query param, falling back to the `Last-Event-ID` header so a
/// browser `EventSource` reconnect resumes where it left off. Held streams share the
/// same global waiter cap as the long-poll; at the cap the caller gets a 503 and falls
/// back to polling.
async fn stream(
    State(state): State<HostedState>,
    Extension(tenant): Extension<Tenant>,
    Path(slug): Path<String>,
    Query(q): Query<ListQuery>,
    headers: HeaderMap,
) -> Response {
    if let Some(resp) = authorize_read(&state, &tenant, &slug) {
        return resp;
    }
    let since = effective_since(q.since, &headers);
    match submissions::open_stream(state.submissions.clone(), slug, since) {
        Some(rx) => submissions::submission_sse(rx).into_response(),
        None => err(
            StatusCode::SERVICE_UNAVAILABLE,
            "too_many_waiters",
            "the server is holding too many streams; retry with the plain poll",
        ),
    }
}

/// The stream cursor: the explicit `since` query param wins; absent, a `Last-Event-ID`
/// header (set by a reconnecting browser `EventSource` from the last event's id) is
/// used; absent or unparseable, start from 0. This only ever affects *which already-
/// persisted* submissions this owner re-reads — key/tenant are still bound server-side
/// — so a malformed value is a harmless full re-read, never a cross-tenant escape.
fn effective_since(query: Option<u64>, headers: &HeaderMap) -> u64 {
    query
        .or_else(|| {
            submissions::parse_last_event_id(
                headers.get("last-event-id").and_then(|v| v.to_str().ok()),
            )
        })
        .unwrap_or(0)
}

/// A tenant may read a page's submissions only when it owns that page. Returns
/// `Some(<404 response>)` to deny (a missing page or a page owned by another tenant
/// both return an opaque "not found" — a tenant learns nothing about pages it does
/// not own), or `None` to allow.
fn authorize_read(state: &HostedState, tenant: &Tenant, slug: &str) -> Option<Response> {
    match state.store.page_tenant(slug) {
        Some(owner) if owner == tenant.0 => None,
        _ => Some(err(
            StatusCode::NOT_FOUND,
            "no_such_page",
            "no such page for this tenant",
        )),
    }
}

/// Clamp the requested long-poll timeout into `(0, MAX_WAIT_SECS]`, defaulting when
/// unset, so a held connection is always bounded.
fn clamp_timeout(secs: Option<u64>) -> std::time::Duration {
    let s = secs.unwrap_or(DEFAULT_WAIT_SECS).clamp(1, MAX_WAIT_SECS);
    std::time::Duration::from_secs(s)
}

/// True only when the request carries an `Origin` that exactly matches an allowed
/// origin. **Fail-closed**: the legitimate caller is always the trusted shell, whose
/// `fetch` POST always sets `Origin` to the served origin, so a **missing** `Origin`
/// (or a foreign one) is rejected — the CSRF boundary. (`allowed` is the server's own
/// canonical origin, so a browser's default-port form like `https://host` matches the
/// canonicalized `https://host` without a port; see `validate_public_origin`.)
pub fn origin_ok(headers: &HeaderMap, allowed: &[String]) -> bool {
    match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(o) => allowed.iter().any(|a| a == o),
        None => false,
    }
}

fn list_response(page: &submissions::ListPage, timed_out: bool) -> Response {
    let items: Vec<serde_json::Value> = page
        .submissions
        .iter()
        .map(|s| s.to_public_json())
        .collect();
    axum::Json(json!({
        "schema_version": SCHEMA_VERSION,
        "submissions": items,
        "cursor": page.cursor,
        "timed_out": timed_out,
    }))
    .into_response()
}

fn submit_error(e: SubmitError) -> Response {
    match e {
        SubmitError::TooLarge => err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "submission_too_large",
            &e.to_string(),
        ),
        SubmitError::Full => err(
            StatusCode::INSUFFICIENT_STORAGE,
            "submissions_full",
            &e.to_string(),
        ),
        SubmitError::RateLimited => err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            &e.to_string(),
        ),
        SubmitError::Io(io) => {
            eprintln!("glasspad host: submit storage error: {io}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not persist the submission",
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
