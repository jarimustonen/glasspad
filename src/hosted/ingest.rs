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
//!
//! Optional `"idempotency_key"` makes the publish exactly-once for the
//! authenticated tenant: a repeat with the same key returns the first page (`200`)
//! instead of minting a new one (`201`). See [`crate::hosted::store`] for the
//! durability + per-tenant-isolation contract.

use axum::extract::rejection::JsonRejection;
use axum::{
    Extension,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

use base64::Engine as _;

use crate::artifact_host::render;
use crate::artifact_host::space::{self, BundleAsset, BundlePage, build_space_bundle};
use crate::cli::SCHEMA_VERSION;
use crate::server::enforce_body_cap;

use super::HostedState;
use super::auth::Tenant;
use super::store::PublishError;

/// Upper bound on an `idempotency_key` (characters). Keys are short deterministic
/// caller-chosen strings; a longer value is rejected (AI-first §strict validation)
/// rather than silently truncated or hashed regardless.
pub const MAX_IDEMPOTENCY_KEY_CHARS: usize = 256;

/// The ingest request body. `deny_unknown_fields` is deliberate: a misspelled
/// `idempotency_key` (e.g. camelCase or hyphenated) would otherwise be silently
/// dropped and defeat the exactly-once contract — instead it is a `400`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishRequest {
    html: Option<String>,
    markdown: Option<String>,
    template: Option<String>,
    title: Option<String>,
    idempotency_key: Option<String>,
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

    // Validate the optional idempotency key: present means non-empty and bounded.
    // (A whitespace-only key is a client bug — reject it rather than silently
    // treating it as "no key".)
    let idempotency_key = match &req.idempotency_key {
        None => None,
        Some(k) => {
            if k.trim().is_empty() {
                return err(
                    StatusCode::BAD_REQUEST,
                    "idempotency_key_empty",
                    "idempotency_key must be a non-empty string when provided",
                );
            }
            if k.chars().count() > MAX_IDEMPOTENCY_KEY_CHARS {
                return err(
                    StatusCode::BAD_REQUEST,
                    "idempotency_key_too_long",
                    &format!("idempotency_key exceeds {MAX_IDEMPOTENCY_KEY_CHARS} characters"),
                );
            }
            Some(k.clone())
        }
    };

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
    let result = tokio::task::spawn_blocking(move || {
        store.publish(&tenant_id, html, title, idempotency_key.as_deref())
    })
    .await;
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
            // A fresh page is `201 Created`; an idempotency-key replay of an
            // already-published page is `200 OK` (same page, not newly created).
            let status = if published.created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            let payload = json!({
                "schema_version": SCHEMA_VERSION,
                "slug": published.slug,
                "url": url,
                "title": published.title,
                "warnings": [],
            });
            (status, axum::Json(payload)).into_response()
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
    build_artifact_body(
        req.html.as_deref(),
        req.markdown.as_deref(),
        req.template.as_deref(),
    )
}

/// Build an artifact body from the same `{html | markdown (+ template)}` inputs the
/// ingest surface accepts, enforcing "exactly one of html/markdown" and the size
/// caps. Shared by `publish` (a new page) and the B2 round-push (a re-render of an
/// existing page) so both go through one validated seam.
pub(crate) fn build_artifact_body(
    html: Option<&str>,
    markdown: Option<&str>,
    template: Option<&str>,
) -> Result<String, (&'static str, String)> {
    let has_html = html.is_some_and(|h| !h.trim().is_empty());
    let has_md = markdown.is_some_and(|m| !m.trim().is_empty());

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
            let html = html.unwrap().to_string();
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
            let md = markdown.unwrap().to_string();
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
            let template = resolve_template(template)?;
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

// --- space ingest (Gap 1: multi-page hosted publish) -----------------------

/// One page in a space-ingest bundle: an already-final HTML artifact body plus its
/// slug (filename stem). `deny_unknown_fields` so a mistyped field is a `400`, never
/// silently dropped.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpacePageInput {
    slug: String,
    html: String,
}

/// One static asset in a space-ingest bundle. `path` is the asset path *relative to*
/// `assets/` (no prefix); `content_base64` is its bytes, standard base64.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpaceAssetInput {
    path: String,
    content_base64: String,
}

/// `POST /api/v1/spaces` request: a whole space (a directory of linked `.html`
/// artifacts) as one bundle. `deny_unknown_fields` guards the stable-key contract
/// exactly as the single-page surface does.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpacePublishRequest {
    pages: Vec<SpacePageInput>,
    #[serde(default)]
    assets: Vec<SpaceAssetInput>,
    #[serde(default)]
    nav: Vec<String>,
    title: Option<String>,
    /// Optional emoji favicon for the space's OUTER shell document. `#[serde(default)]`
    /// so an older producer that omits it still parses. Validated server-side (the
    /// untrusted API boundary) before it is stored / rendered.
    #[serde(default)]
    favicon: Option<String>,
    /// Stable space key: a re-publish with the same key updates the space **in
    /// place** at the same slug/URL (owner-scoped). Absent → a fresh slug.
    space_key: Option<String>,
}

/// `POST /api/v1/spaces`. Ingest a whole space into one hosted namespace
/// `/p/<slug>/…` with in-space bridge nav + relative links resolving across pages.
/// The [`Tenant`] is injected by the auth middleware (never from the body). On
/// success returns `201` (fresh) / `200` (stable-key update-in-place) with the space
/// slug, url, and a per-page list. Reuses the exact sandbox/CSP read seam — every
/// page is served as a null-origin sandboxed iframe, unchanged.
pub async fn publish_space(
    State(state): State<HostedState>,
    Extension(tenant): Extension<Tenant>,
    body: Result<axum::Json<SpacePublishRequest>, JsonRejection>,
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

    // Bound the optional space title (same cap as a page title).
    if let Some(t) = &req.title
        && t.chars().count() > space::MAX_TITLE_CHARS
    {
        return err(
            StatusCode::BAD_REQUEST,
            "title_too_long",
            &format!("title exceeds {} characters", space::MAX_TITLE_CHARS),
        );
    }

    // Validate the optional stable space key (non-empty, bounded) — same rules as an
    // idempotency key.
    let space_key = match &req.space_key {
        None => None,
        Some(k) => {
            if k.trim().is_empty() {
                return err(
                    StatusCode::BAD_REQUEST,
                    "space_key_empty",
                    "space_key must be a non-empty string when provided",
                );
            }
            if k.chars().count() > MAX_IDEMPOTENCY_KEY_CHARS {
                return err(
                    StatusCode::BAD_REQUEST,
                    "space_key_too_long",
                    &format!("space_key exceeds {MAX_IDEMPOTENCY_KEY_CHARS} characters"),
                );
            }
            Some(k.clone())
        }
    };

    // Decode assets (base64) and shape the bundle inputs. The per-file / per-space
    // byte caps are enforced by `build_space_bundle`; here we only reject an asset
    // whose base64 is malformed (a caller bug, not silently dropped).
    let pages: Vec<BundlePage> = req
        .pages
        .into_iter()
        .map(|p| BundlePage {
            slug: p.slug,
            html: p.html,
        })
        .collect();
    let mut assets: Vec<BundleAsset> = Vec::with_capacity(req.assets.len());
    for a in req.assets {
        let bytes =
            match base64::engine::general_purpose::STANDARD.decode(a.content_base64.as_bytes()) {
                Ok(b) => b,
                Err(e) => {
                    return err(
                        StatusCode::BAD_REQUEST,
                        "bad_asset_base64",
                        &format!("asset {:?} has invalid base64 content: {e}", a.path),
                    );
                }
            };
        assets.push(BundleAsset {
            path: a.path,
            bytes,
        });
    }

    // Validate the optional favicon at the untrusted API boundary — a non-emoji /
    // injection value is rejected here (never stored / rendered), the authoritative
    // check on top of the producer's own CLI-side validation (AI-first §1).
    let favicon = match &req.favicon {
        None => None,
        Some(raw) => match crate::favicon::validate(raw) {
            Ok(v) => Some(v),
            Err(msg) => return err(StatusCode::BAD_REQUEST, "invalid_favicon", &msg),
        },
    };

    // Build + validate the space with the SAME rules the filesystem scanner applies.
    let mut space = match build_space_bundle(pages, assets, req.nav, req.title) {
        Ok(sp) => sp,
        Err(e) => return err(StatusCode::BAD_REQUEST, "invalid_space", &e.to_string()),
    };
    space.favicon = favicon;

    // Blocking filesystem I/O + store mutation lock → off the async worker.
    let store = state.store.clone();
    let tenant_id = tenant.0.clone();
    let key = space_key.clone();
    let result =
        tokio::task::spawn_blocking(move || store.publish_space(&tenant_id, space, key.as_deref()))
            .await;
    let result = match result {
        Ok(r) => r,
        Err(join) => {
            eprintln!("glasspad host: publish_space task panicked: {join}");
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not persist the space",
            );
        }
    };

    match result {
        Ok(published) => {
            let space_url = format!("{}{}/{}/", state.public_origin, state.mount, published.slug);
            let pages_json: Vec<serde_json::Value> = published
                .pages
                .iter()
                .map(|p| {
                    json!({
                        "slug": p.slug,
                        "title": p.title,
                        "url": format!(
                            "{}{}/{}/{}",
                            state.public_origin, state.mount, published.slug, p.slug
                        ),
                    })
                })
                .collect();
            let status = if published.created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            let payload = json!({
                "schema_version": SCHEMA_VERSION,
                "slug": published.slug,
                "url": space_url,
                "title": published.title,
                "home": published.home,
                "pages": pages_json,
                "page_count": published.pages.len(),
                "created": published.created,
                "warnings": [],
            });
            (status, axum::Json(payload)).into_response()
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
            eprintln!("glasspad host: space ingest storage error: {e}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "could not persist the space",
            )
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
