use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::{Router, middleware};
use tokio::net::TcpListener;

use crate::artifact_host::space::{self, Artifact, ScanError, Snapshot, Space};
use crate::artifact_host::{self, ArtifactHost, guards};

/// How often the (dependency-free) filesystem watcher polls the served directory
/// for changes. 500 ms is imperceptible for a local edit-reload loop and avoids
/// pulling in a native file-notification dependency for a localhost dev tool.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);

/// Build the complete, fully-guarded application router over a shared artifact
/// host. Extracted so tests can exercise the middleware stack (the global Host
/// guard) — `artifact_host::router` alone omits it, which would let the security
/// gate pass with the guard absent or misordered.
///
/// The v0.1 control API (`/api/pads`) and legacy `/{id}` pad renderer were
/// removed in Wave 3 (design.md §10, decision D2): the only same-origin surface
/// now is the sandboxed artifact host, so the sole coexistence risk it posed is
/// closed.
pub fn build_app_with_host(port: u16, host: Arc<ArtifactHost>) -> Router {
    // --- v0.2 HTML-artifact host (Wave 1 security gate + Wave 2a space model) ---
    artifact_host::router(host)
        // Global DNS-rebinding defense: validate the Host header on every route.
        .layer(middleware::from_fn_with_state(port, guards::host_guard))
}

/// Convenience for tests that don't serve a live directory (fixtures only).
#[cfg(test)]
pub fn build_app(port: u16) -> Router {
    build_app_with_host(port, Arc::new(ArtifactHost::new(port)))
}

/// The slug of the single artifact a `create`d space holds: its canonical home.
pub const SINGLE_SLUG: &str = "index";

/// Bind the loopback control plane. **Loopback-only** (design.md §5): binding a
/// routable interface is not offered without an explicit unsafe opt-in (not
/// implemented). Returns the bind error so the CLI can surface it as its error
/// envelope (e.g. port already in use) rather than panicking.
pub async fn bind_loopback(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port)).await
}

/// Serve the app on an already-bound listener until the process is killed. Split
/// from `bind_loopback` so the CLI can bind first (surfacing a bind failure as an
/// error) and print its startup envelope only once the port is actually held.
/// Returns the serve error instead of panicking, so the CLI can surface a
/// mid-run failure as its structured error envelope (AI-first §10).
pub async fn serve_on(listener: TcpListener, app: Router) -> std::io::Result<()> {
    axum::serve(listener, app).await
}

/// Scan `dir` into a one-space [`Snapshot`], also returning the derived space
/// name. The name is the directory's final component, validated against the space
/// grammar + reserved list. Fail-fast: a malformed / colliding / reserved space
/// is an error the caller reports informatively (AI-first CLI contract).
pub fn scan_named(dir: &Path) -> Result<(String, Snapshot), ScanError> {
    let name = space::space_name_for(dir)?;
    let space = space::scan_dir(dir)?;
    let mut snap = Snapshot::empty();
    snap.spaces.insert(name.clone(), space);
    Ok((name, snap))
}

/// Scan `dir` into a one-space [`Snapshot`] (the name is discarded — used by the
/// watcher, which already knows the directory it is re-scanning).
pub fn load_space(dir: &Path) -> Result<Snapshot, ScanError> {
    Ok(scan_named(dir)?.1)
}

/// Build a one-artifact snapshot from a single file's HTML (the `create` model).
/// The lone artifact is the space's home (`SINGLE_SLUG`); its title is resolved
/// from the HTML (`<title>`/`<h1>`, parsed not regexed), falling back to the space
/// name. Fragment-vs-full-document detection is **not** done here: the raw HTML is
/// stored verbatim and the content route classifies + wraps it at serve time
/// (`wrap::render_artifact`), so `create` and `serve` share one detector.
pub fn one_artifact_snapshot(name: &str, html: String) -> Snapshot {
    let title = space::resolve_title(&html).unwrap_or_else(|| name.to_string());
    let mut sp = Space::default();
    sp.artifacts
        .insert(SINGLE_SLUG.to_string(), Artifact { html, title });
    sp.nav = vec![SINGLE_SLUG.to_string()];
    sp.home = Some(SINGLE_SLUG.to_string());
    let mut snap = Snapshot::empty();
    snap.spaces.insert(name.to_string(), sp);
    snap
}

/// A dependency-free filesystem watcher: poll a cheap fingerprint of the scan
/// surface and, on change, rescan into a fresh snapshot, swap it atomically, and
/// fire the SSE reload. Runs the blocking fingerprint + scan on a blocking pool
/// (never an async worker). A rescan that fails (e.g. the user just introduced a
/// slug collision) keeps the last-good snapshot serving and is retried when the
/// surface changes again; the same failure is logged only once.
pub fn spawn_watcher(host: Arc<ArtifactHost>, dir: PathBuf) {
    tokio::spawn(async move {
        // `loaded_fp` tracks the last *successfully loaded* state, so a failed
        // scan is retried on the next tick instead of being silently skipped.
        let mut loaded_fp = fp_blocking(dir.clone()).await;
        let mut last_err_fp: Option<u64> = None;
        loop {
            tokio::time::sleep(WATCH_INTERVAL).await;
            let fp = fp_blocking(dir.clone()).await;
            if fp == loaded_fp {
                continue;
            }
            let d = dir.clone();
            match tokio::task::spawn_blocking(move || load_space(&d)).await {
                Ok(Ok(snap)) => {
                    host.swap(snap);
                    host.notify_reload();
                    loaded_fp = fp;
                    last_err_fp = None;
                    eprintln!("glasspad: reloaded {}", dir.display());
                }
                Ok(Err(e)) => {
                    // Keep serving the last-good snapshot; retry when the surface
                    // changes. Log each distinct failing state once (no 2 Hz spam).
                    if last_err_fp != Some(fp) {
                        eprintln!(
                            "glasspad: rescan of {} failed, keeping last good snapshot: {e}",
                            dir.display()
                        );
                        last_err_fp = Some(fp);
                    }
                }
                Err(join) => eprintln!("glasspad: watcher task failed: {join}"),
            }
        }
    });
}

/// The single-file analogue of [`spawn_watcher`] (the `create` model): poll one
/// file's `(len, mtime)` and, on change, re-read it into a fresh one-artifact
/// snapshot, swap atomically, and fire the SSE reload. A read that fails (file
/// removed, non-UTF-8, over the per-file cap) keeps the last-good snapshot serving
/// and is logged once, so a single-file edit loop reloads the browser just like
/// `serve ./dir` while a transient bad save never blanks the page.
pub fn spawn_file_watcher(host: Arc<ArtifactHost>, file: PathBuf, name: String) {
    tokio::spawn(async move {
        let mut loaded_fp = file_fp_blocking(file.clone()).await;
        let mut last_err_fp: Option<(u64, i128)> = None;
        loop {
            tokio::time::sleep(WATCH_INTERVAL).await;
            let fp = file_fp_blocking(file.clone()).await;
            if fp == loaded_fp {
                continue;
            }
            let f = file.clone();
            match tokio::task::spawn_blocking(move || read_artifact_file(&f)).await {
                Ok(Ok(html)) => {
                    host.swap(one_artifact_snapshot(&name, html));
                    host.notify_reload();
                    loaded_fp = fp;
                    last_err_fp = None;
                    eprintln!("glasspad: reloaded {}", file.display());
                }
                Ok(Err(e)) => {
                    if last_err_fp != Some(fp) {
                        eprintln!(
                            "glasspad: reload of {} failed, keeping last good content: {e}",
                            file.display()
                        );
                        last_err_fp = Some(fp);
                    }
                }
                Err(join) => eprintln!("glasspad: file watcher task failed: {join}"),
            }
        }
    });
}

/// Read a single artifact file for the `create` watcher: reject a non-regular
/// file or one over the per-file cap **before** reading it all, then require
/// UTF-8. Returns an informative message on failure (logged, not fatal — the
/// watcher keeps the last-good snapshot). The initial `create` load does its own
/// richer validation (see `cli`); this is the reload path.
pub fn read_artifact_file(file: &Path) -> Result<String, String> {
    use std::io::Read;
    let meta = std::fs::metadata(file).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err(format!("{} is not a regular file", file.display()));
    }
    if meta.len() > space::MAX_FILE_BYTES {
        return Err(format!(
            "{} bytes, over the {}-byte per-file limit",
            meta.len(),
            space::MAX_FILE_BYTES
        ));
    }
    // Bounded read (cap the allocation at limit + 1) so a file that grows past the
    // cap between the stat above and the read cannot force an unbounded buffer.
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    f.take(space::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > space::MAX_FILE_BYTES {
        return Err(format!(
            "over the {}-byte per-file limit",
            space::MAX_FILE_BYTES
        ));
    }
    String::from_utf8(bytes).map_err(|_| format!("{} is not valid UTF-8", file.display()))
}

/// Run [`file_fp`] on the blocking pool.
async fn file_fp_blocking(file: PathBuf) -> (u64, i128) {
    tokio::task::spawn_blocking(move || file_fp(&file))
        .await
        .unwrap_or((0, -1))
}

/// A single file's change fingerprint: `(len, mtime_nanos)`. Follows symlinks
/// (`metadata`, not `symlink_metadata`) — `create` serves the file the user named
/// even if it is a symlink to their own file, so the watch tracks the target.
fn file_fp(file: &Path) -> (u64, i128) {
    match std::fs::metadata(file) {
        Ok(m) => {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as i128)
                .unwrap_or(-1);
            (m.len(), mtime)
        }
        Err(_) => (0, -1),
    }
}

/// Run `fingerprint` on the blocking pool.
async fn fp_blocking(dir: PathBuf) -> u64 {
    tokio::task::spawn_blocking(move || fingerprint(&dir))
        .await
        .unwrap_or(0)
}

/// A cheap change-detection fingerprint over **exactly the scan surface**: the
/// top-level directory listing (so any added/removed/edited top-level file or the
/// manifest is caught) plus the `assets/` subtree recursively. It deliberately
/// does **not** descend into other subdirectories (`.git`, `node_modules`, build
/// output) — the scanner ignores them, so walking them every tick would be wasted
/// CPU. Never follows symlinks (a symlink's own metadata is hashed, so swapping a
/// file for a symlink is detected).
fn fingerprint(dir: &Path) -> u64 {
    let mut entries: Vec<(PathBuf, bool, u64, i128)> = Vec::new();
    collect_level(dir, false, &mut entries); // top level only
    collect_level(&dir.join(space::ASSETS_DIR), true, &mut entries); // assets subtree
    entries.sort();
    let mut hasher = DefaultHasher::new();
    entries.hash(&mut hasher);
    hasher.finish()
}

/// Collect `(path, is_symlink, len, mtime_nanos)` for one directory. When
/// `recurse` is set, descend into real subdirectories (used for `assets/`).
fn collect_level(dir: &Path, recurse: bool, out: &mut Vec<(PathBuf, bool, u64, i128)>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_symlink = meta.file_type().is_symlink();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i128)
            .unwrap_or(-1);
        out.push((path.clone(), is_symlink, meta.len(), mtime));
        if recurse && meta.is_dir() && !is_symlink {
            collect_level(&path, true, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::util::ServiceExt;

    fn app() -> Router {
        build_app(3000)
    }

    async fn send(req: Request<Body>) -> StatusCode {
        app().oneshot(req).await.unwrap().status()
    }

    /// Wave 3 (D2) invariant: the legacy same-origin control surface is gone and
    /// no *new* same-origin mutation endpoint has crept back in. This is what
    /// makes it safe to have unwired `control_origin_guard` — there is nothing
    /// state-mutating for it to protect. If a future wave adds a `POST`/`PUT`/
    /// `DELETE` control route, this test fails until Origin protection is wired,
    /// forcing the guard back on before the endpoint ships.
    #[tokio::test]
    async fn no_mutating_same_origin_surface_exists() {
        // Neither the removed legacy `/api/pads` CRUD surface nor any mutating
        // method against a live artifact route is *handled*: every one bounces
        // with 404 (no such route) or 405 (the artifact routes are GET-only, so
        // `/api/pads` now merely falls through to `/{space}/{slug}` and a
        // mutating verb is rejected). A 2xx here would mean a same-origin write
        // path exists — the thing the unwired `control_origin_guard` would need
        // to protect. The invariant that keeps it safely unwired is: there is
        // none.
        let cases = [
            (Method::GET, "/api/pads"),
            (Method::POST, "/api/pads"),
            (Method::PUT, "/api/pads/abc"),
            (Method::DELETE, "/api/pads/abc"),
            (Method::POST, "/demo/_c/index"),
            (Method::PUT, "/demo/_c/index"),
            (Method::DELETE, "/demo/_c/index"),
            (Method::PATCH, "/demo/_c/index"),
        ];
        for (method, uri) in cases {
            let s = send(
                Request::builder()
                    .method(method.clone())
                    .uri(uri)
                    .header("host", "127.0.0.1:3000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert!(
                matches!(s, StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED),
                "{method} {uri} was handled ({s}) — an unguarded same-origin \
                 mutation surface may have been (re)introduced"
            );
        }
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
}
