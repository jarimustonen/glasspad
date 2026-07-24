use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tokio::net::TcpListener;

use crate::artifact_host::space::{self, ScanError, Snapshot};
use crate::artifact_host::{self, guards, ArtifactHost};
use crate::routes::{api, render};
use crate::store::PadStore;

/// How often the (dependency-free) filesystem watcher polls the served directory
/// for changes. 500 ms is imperceptible for a local edit-reload loop and avoids
/// pulling in a native file-notification dependency for a localhost dev tool.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);

/// Build the complete, fully-guarded application router over a shared artifact
/// host. Extracted so tests can exercise the middleware stack (Host / Origin
/// guards) — `artifact_host::router` alone omits them, which would let the
/// security gate pass with the guards absent or misordered.
pub fn build_app_with_host(port: u16, store: Arc<PadStore>, host: Arc<ArtifactHost>) -> Router {
    // --- v0.1 control API + legacy pad render (unchanged; removed in Wave 5) ---
    // The control API is guarded per design.md §5: reject `Origin: null` and any
    // foreign origin on control endpoints.
    let control = Router::new()
        .route("/api/pads", post(api::create_pad).get(api::list_pads))
        .route(
            "/api/pads/{id}",
            get(api::get_pad)
                .put(api::update_pad)
                .delete(api::delete_pad),
        )
        .route_layer(middleware::from_fn_with_state(
            port,
            guards::control_origin_guard,
        ))
        .route("/{id}", get(render::get_pad_html))
        .with_state(store);

    // --- v0.2 HTML-artifact host (Wave 1 security gate + Wave 2a space model) ---
    control
        .merge(artifact_host::router(host))
        // Global DNS-rebinding defense: validate the Host header on every route.
        .layer(middleware::from_fn_with_state(port, guards::host_guard))
}

/// Convenience for tests that don't serve a live directory (fixtures only).
#[cfg(test)]
pub fn build_app(port: u16, store: Arc<PadStore>) -> Router {
    build_app_with_host(port, store, Arc::new(ArtifactHost::new(port)))
}

/// Serve, optionally rendering a live directory as a space (Wave 2a). Wave 3a
/// formalizes the CLI surface (`serve ./dir`, fragment detection, `--json`); this
/// is the minimal wiring Phase 2 needs to prove live serving end-to-end.
pub async fn run_dir(port: u16, dir: Option<PathBuf>) {
    let store = Arc::new(PadStore::new(port));
    let host = Arc::new(ArtifactHost::new(port));

    if let Some(dir) = dir {
        // Initial scan is fail-fast: a malformed/colliding/reserved space is an
        // error the user must fix, reported informatively (AI-first CLI contract).
        match load_space(&dir) {
            Ok(snap) => host.swap(snap),
            Err(e) => {
                eprintln!("glasspad: cannot serve {}: {e}", dir.display());
                std::process::exit(1);
            }
        }
        spawn_watcher(host.clone(), dir);
    }

    let app = build_app_with_host(port, store, host);

    let addr = format!("127.0.0.1:{}", port);
    eprintln!("glasspad serving on http://{}", addr);

    // Loopback-only bind (design.md §5). Binding a routable interface is not
    // offered without an explicit unsafe opt-in (not implemented yet).
    let listener = TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Scan `dir` into a one-space [`Snapshot`]. The space name is the directory's
/// final component, validated against the space grammar + reserved list.
pub fn load_space(dir: &Path) -> Result<Snapshot, ScanError> {
    let name = space::space_name_for(dir)?;
    let space = space::scan_dir(dir)?;
    let mut snap = Snapshot::empty();
    snap.spaces.insert(name, space);
    Ok(snap)
}

/// A dependency-free filesystem watcher: poll a cheap fingerprint of the tree
/// (paths + mtimes + sizes) and, on any change, rescan into a fresh snapshot,
/// swap it atomically, and fire the SSE reload. A rescan that fails (e.g. the
/// user just introduced a slug collision) keeps the last-good snapshot serving.
fn spawn_watcher(host: Arc<ArtifactHost>, dir: PathBuf) {
    tokio::spawn(async move {
        let mut last = fingerprint(&dir);
        loop {
            tokio::time::sleep(WATCH_INTERVAL).await;
            let fp = fingerprint(&dir);
            if fp == last {
                continue;
            }
            last = fp;
            match load_space(&dir) {
                Ok(snap) => {
                    host.swap(snap);
                    host.notify_reload();
                    eprintln!("glasspad: reloaded {}", dir.display());
                }
                Err(e) => {
                    eprintln!(
                        "glasspad: rescan of {} failed, keeping last good snapshot: {e}",
                        dir.display()
                    );
                }
            }
        }
    });
}

/// A cheap change-detection fingerprint over the whole tree: every entry's
/// relative path, size, and mtime. Never follows symlinks (a symlink's own
/// metadata is hashed, so swapping a file for a symlink is detected). Errors
/// collapse to a sentinel so a transient read failure just triggers a rescan.
fn fingerprint(dir: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut stack = vec![dir.to_path_buf()];
    let mut entries: Vec<(PathBuf, bool, u64, i128)> = Vec::new();
    while let Some(cur) = stack.pop() {
        let rd = match std::fs::read_dir(&cur) {
            Ok(rd) => rd,
            Err(_) => {
                0u8.hash(&mut hasher);
                continue;
            }
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let is_symlink = meta.file_type().is_symlink();
            if meta.is_dir() && !is_symlink {
                stack.push(path.clone());
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as i128)
                .unwrap_or(-1);
            entries.push((path, is_symlink, meta.len(), mtime));
        }
    }
    // Sort for a stable, order-independent fingerprint.
    entries.sort();
    entries.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    fn app() -> Router {
        build_app(3000, Arc::new(PadStore::new(3000)))
    }

    async fn send(req: Request<Body>) -> StatusCode {
        app().oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn host_guard_accepts_loopback() {
        let s = send(
            Request::get("/demo/_c/index")
                .header("host", "127.0.0.1:3000")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let s = send(
            Request::get("/demo/_c/index")
                .header("host", "localhost:3000")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }

    #[tokio::test]
    async fn host_guard_rejects_rebinding_and_missing() {
        // DNS-rebinding attacker Host.
        let s = send(
            Request::get("/demo/_c/index")
                .header("host", "attacker.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::MISDIRECTED_REQUEST);
        // Foreign port.
        let s = send(
            Request::get("/demo/_c/index")
                .header("host", "127.0.0.1:9999")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::MISDIRECTED_REQUEST);
    }

    #[tokio::test]
    async fn control_origin_guard_rejects_null_and_foreign_on_api() {
        // Sandboxed-frame / cross-origin write carries Origin: null → rejected.
        let s = send(
            Request::post("/api/pads")
                .header("host", "127.0.0.1:3000")
                .header("origin", "null")
                .header("content-type", "application/x-yaml")
                .body(Body::from("spec_version: 1\ntitle: x\nsections: []\n"))
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let s = send(
            Request::post("/api/pads")
                .header("host", "127.0.0.1:3000")
                .header("origin", "http://evil.example.com")
                .header("content-type", "application/x-yaml")
                .body(Body::from("spec_version: 1\ntitle: x\nsections: []\n"))
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn control_origin_guard_allows_loopback_origin() {
        // Same-loopback-origin request passes the Origin guard (content-type
        // then drives the rest of the handler; we only assert it's not FORBIDDEN).
        let s = send(
            Request::get("/api/pads")
                .header("host", "127.0.0.1:3000")
                .header("origin", "http://127.0.0.1:3000")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
}
