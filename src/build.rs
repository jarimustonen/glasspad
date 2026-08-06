//! `glasspad build` — static, self-contained render of a space to HTML files.
//!
//! A space directory is scanned by the **same security-checked scanner** the
//! server uses (`space::scan_dir` via `server::scan_named`: symlink / traversal /
//! reserved-slug / collision / size are all rejected before a single byte is
//! written), then each artifact is wrapped through the **same render seam** the
//! content route uses (`wrap::render_artifact`): a fragment is wrapped into a
//! themed document with `base.css` linked + `bridge.js` injected; a full document
//! is emitted verbatim. The renderer is **not** forked — the build produces the
//! same wrapped bytes the server would serve, written to files instead.
//!
//! There is no server, no bind, and no per-response CSP header here: a static file
//! carries no HTTP headers, so the null-origin sandbox contract the live host
//! enforces does not apply to build output. That is an accepted property of static
//! render — the output is for an **offline docsite / external preview transport**,
//! where the input-side guarantees (the scanner refusing hostile inputs) are what
//! carry over, not the response-side CSP/sandbox.
//!
//! ## Base-lib handling (self-contained vs shared-libs)
//!
//! * **self-contained** (default, offline-safe): the pinned base libs
//!   ([`fixtures::BASE_LIB_NAMES`]) are copied under `_gp/v1/`, and the two refs
//!   `wrap` injects into a fragment (`base.css` / `bridge.js`) are rewritten from
//!   the absolute server path `/_gp/v1/…` to a **relative** `_gp/v1/…` so a wrapped
//!   page resolves them whether opened via `file://` or served from the output
//!   root. (A full document is emitted verbatim — its author owns its own paths.)
//! * **shared-libs**: the libs are neither copied nor rewritten; a wrapped page
//!   keeps `wrap`'s absolute `/_gp/v1/…` refs, to be resolved by whatever serves
//!   the output at its origin root (a running glasspad or a host that mirrors
//!   `_gp/v1/`). Smaller output, not standalone.

use std::path::Path;

use crate::artifact_host::fixtures;
use crate::artifact_host::space::Space;
use crate::artifact_host::wrap::{self, Theme};

/// How a build treats the pinned base libraries (`base.css` / `bridge.js` / …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibMode {
    /// Copy the libs under `_gp/v1/` and reference them **relatively** so the
    /// output is standalone (works offline, no running host). The default.
    SelfContained,
    /// Reference the libs at the canonical absolute `/_gp/v1/…` server path and do
    /// **not** copy them — resolved by whatever serves the output at its root.
    SharedLibs,
}

impl LibMode {
    /// The stable token used in the `--json` envelope (`mode` field).
    pub fn as_str(self) -> &'static str {
        match self {
            LibMode::SelfContained => "self-contained",
            LibMode::SharedLibs => "shared-libs",
        }
    }
}

/// One file the build emits: a path **relative to the output directory** (always
/// `/`-separated, never absolute, never containing `..`) plus its bytes.
#[derive(Clone, Debug)]
pub struct OutFile {
    pub rel_path: String,
    pub bytes: Vec<u8>,
}

/// Wrap one artifact into the page bytes the build writes for it. Reuses the
/// content route's seam (`wrap::render_artifact` at the FOUC-free `auto` theme):
/// a fragment is wrapped + bridged, a full document is passed through verbatim. In
/// [`LibMode::SelfContained`] the two refs `wrap` injects into a **fragment** are
/// localized to a relative `_gp/v1/…` path (see [`localize_base_libs`]); a full
/// document is never rewritten (no injected refs to rewrite — the author owns it).
pub fn wrapped_page(artifact_html: &str, mode: LibMode) -> String {
    let out = wrap::render_artifact(artifact_html, Theme::Auto);
    if mode == LibMode::SelfContained && wrap::is_fragment(artifact_html) {
        localize_base_libs(out)
    } else {
        out
    }
}

/// Rewrite the two base-lib refs `wrap::wrap_fragment` injects from the absolute
/// server path (`/_gp/v1/…`) to a **relative** one (`_gp/v1/…`). Applied only to
/// wrapped **fragments** (where these exact tags are first-party wrap output, never
/// author bytes), so it can never touch an artifact author's intentionally-absolute
/// reference. The match is exact against `wrap`'s emitted tags; the build's tests
/// assert the rewrite actually fired, so a change to `wrap`'s tag formatting fails
/// loudly rather than silently leaving an unresolvable path.
fn localize_base_libs(wrapped: String) -> String {
    wrapped
        .replace("href=\"/_gp/v1/base.css\"", "href=\"_gp/v1/base.css\"")
        .replace("src=\"/_gp/v1/bridge.js\"", "src=\"_gp/v1/bridge.js\"")
}

/// A minimal `index.html` that redirects to `home` (used when the space's home
/// slug is not literally `index`, so the output still has a canonical entry point).
/// `home` is a validated slug (`[a-z0-9][a-z0-9-]*`), so it needs no escaping — the
/// grammar excludes every HTML/URL metacharacter.
fn index_redirect(home: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n\
         <meta http-equiv=\"refresh\" content=\"0; url={home}.html\">\n\
         <title>Redirecting…</title>\n</head><body>\n\
         <p><a href=\"{home}.html\">Continue to {home}</a></p>\n</body></html>\n",
        home = home
    )
}

/// Plan every file a build of `space` emits, in deterministic order. Pure — it
/// allocates the output bytes but touches no filesystem, so it is exhaustively
/// unit-testable. The caller ([`write_files`]) is the only thing that does I/O.
///
/// Emits, in order: one `<slug>.html` per artifact (wrapped through
/// [`wrapped_page`]); an `index.html` **redirect** to the home artifact when the
/// home slug is not itself `index` (when it is, its own `index.html` is the entry
/// point); every static asset under its space-relative `assets/…` key; and — in
/// [`LibMode::SelfContained`] — the pinned base libs under `_gp/v1/…`.
pub fn plan(space: &Space, home: Option<&str>, mode: LibMode) -> Vec<OutFile> {
    let mut files = Vec::new();

    // One wrapped page per artifact (BTreeMap → deterministic slug order).
    for (slug, artifact) in &space.artifacts {
        files.push(OutFile {
            rel_path: format!("{slug}.html"),
            bytes: wrapped_page(&artifact.html, mode).into_bytes(),
        });
    }

    // A canonical `index.html` entry point. If an artifact is literally named
    // `index`, its own page already is `index.html`; otherwise emit a redirect to
    // the resolved home so opening the output directory lands somewhere sensible.
    if let Some(home) = home
        && !space.artifacts.contains_key("index")
    {
        files.push(OutFile {
            rel_path: "index.html".to_string(),
            bytes: index_redirect(home).into_bytes(),
        });
    }

    // Static assets, copied verbatim under their space-relative `assets/…` key.
    for (key, asset) in &space.assets {
        files.push(OutFile {
            rel_path: key.clone(),
            bytes: asset.bytes.clone(),
        });
    }

    // Self-contained: bundle the pinned base libs so the output resolves them with
    // no running host. `gp_asset` is the single source of truth for their bytes.
    if mode == LibMode::SelfContained {
        for name in fixtures::BASE_LIB_NAMES {
            if let Some((_, body)) = fixtures::gp_asset(name) {
                files.push(OutFile {
                    rel_path: format!("_gp/v1/{name}"),
                    bytes: body.as_bytes().to_vec(),
                });
            }
        }
    }

    files
}

/// Write a planned file set under `out`, creating parent directories as needed.
/// `out` is created if absent. Each `rel_path` is a build-produced relative,
/// `..`-free, `/`-separated path (see [`OutFile`]), joined onto `out` with the
/// platform separator — no traversal is possible from these first-party paths.
pub fn write_files(out: &Path, files: &[OutFile]) -> std::io::Result<()> {
    std::fs::create_dir_all(out)?;
    for f in files {
        // Join each `/`-separated segment so the relative path maps to the right
        // nested location on any platform (a bare `join` of `"a/b"` is fine on
        // unix; doing it segment-wise is correct everywhere).
        let mut path = out.to_path_buf();
        for seg in f.rel_path.split('/') {
            path.push(seg);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &f.bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact_host::space::{Artifact, Asset};

    fn space_with(artifacts: &[(&str, &str)]) -> Space {
        let mut sp = Space::default();
        for (slug, html) in artifacts {
            sp.artifacts.insert(
                (*slug).to_string(),
                Artifact {
                    html: (*html).to_string(),
                    title: (*slug).to_string(),
                },
            );
        }
        sp.nav = artifacts.iter().map(|(s, _)| (*s).to_string()).collect();
        sp.home = artifacts.first().map(|(s, _)| (*s).to_string());
        sp
    }

    fn find<'a>(files: &'a [OutFile], rel: &str) -> Option<&'a OutFile> {
        files.iter().find(|f| f.rel_path == rel)
    }

    fn text<'a>(files: &'a [OutFile], rel: &str) -> &'a str {
        std::str::from_utf8(&find(files, rel).expect("file present").bytes).unwrap()
    }

    #[test]
    fn fragment_page_is_wrapped_and_bridged() {
        // A fragment flows through the same wrap seam the server uses: doctype +
        // base.css + bridge.js, body preserved.
        let page = wrapped_page("<h1>Hi</h1>", LibMode::SelfContained);
        assert!(page.starts_with("<!doctype html>"));
        assert!(page.contains("<h1>Hi</h1>"));
        assert!(page.contains("bridge.js"));
        assert!(page.contains("base.css"));
    }

    #[test]
    fn full_document_is_served_verbatim_in_both_modes() {
        // A full document is passed through unchanged — no injected bridge, and no
        // localization (the author owns its own paths), in either mode.
        let full = "<!doctype html><html><head>\
                    <link rel=\"stylesheet\" href=\"/_gp/v1/base.css\"></head>\
                    <body><h1>x</h1></body></html>";
        assert_eq!(wrapped_page(full, LibMode::SelfContained), full);
        assert_eq!(wrapped_page(full, LibMode::SharedLibs), full);
    }

    #[test]
    fn self_contained_localizes_fragment_base_libs_to_relative() {
        let page = wrapped_page("<h1>Hi</h1>", LibMode::SelfContained);
        // The wrap-injected refs are rewritten to a relative path…
        assert!(page.contains(r#"href="_gp/v1/base.css""#));
        assert!(page.contains(r#"src="_gp/v1/bridge.js""#));
        // …and the absolute server path is gone (so file:// resolves them).
        assert!(!page.contains(r#"href="/_gp/v1/base.css""#));
        assert!(!page.contains(r#"src="/_gp/v1/bridge.js""#));
    }

    #[test]
    fn shared_libs_keeps_absolute_refs_and_omits_libs() {
        let page = wrapped_page("<h1>Hi</h1>", LibMode::SharedLibs);
        // Absolute server path is kept (resolved by whatever serves the root).
        assert!(page.contains(r#"href="/_gp/v1/base.css""#));
        assert!(page.contains(r#"src="/_gp/v1/bridge.js""#));

        let sp = space_with(&[("index", "<h1>Hi</h1>")]);
        let files = plan(&sp, sp.home.as_deref(), LibMode::SharedLibs);
        // No base libs are bundled in shared-libs mode.
        assert!(!files.iter().any(|f| f.rel_path.starts_with("_gp/v1/")));
    }

    #[test]
    fn self_contained_bundles_libs_that_pages_actually_reference() {
        // The "resolves its base libs offline" contract: a self-contained fragment
        // page references `_gp/v1/base.css` + `_gp/v1/bridge.js`, and both of those
        // exact relative paths are present as real, non-empty files in the plan.
        let sp = space_with(&[("index", "<h1>Hi</h1>")]);
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained);
        let page = text(&files, "index.html");
        for referenced in ["_gp/v1/base.css", "_gp/v1/bridge.js"] {
            assert!(
                page.contains(referenced),
                "page must reference {referenced}"
            );
            let f = find(&files, referenced).expect("referenced lib bundled");
            assert!(!f.bytes.is_empty(), "{referenced} must have real bytes");
        }
        // The rest of the pinned set is bundled too (self-contained incl. charts).
        for name in fixtures::BASE_LIB_NAMES {
            assert!(
                find(&files, &format!("_gp/v1/{name}")).is_some(),
                "{name} bundled"
            );
        }
    }

    #[test]
    fn index_slug_is_the_entry_point_no_redirect() {
        // When an artifact is literally `index`, its own wrapped page is index.html
        // and there is exactly one such file (no extra redirect clobbering it).
        let sp = space_with(&[("index", "<h1>Home</h1>"), ("sales", "<h1>Sales</h1>")]);
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained);
        assert_eq!(
            files.iter().filter(|f| f.rel_path == "index.html").count(),
            1
        );
        assert!(text(&files, "index.html").contains("<h1>Home</h1>"));
        assert!(find(&files, "sales.html").is_some());
    }

    #[test]
    fn non_index_home_gets_a_redirect_index() {
        // Home is `report`, no `index` artifact → an index.html redirect to it.
        let sp = space_with(&[("report", "<h1>Report</h1>"), ("appendix", "<h1>A</h1>")]);
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained);
        let idx = text(&files, "index.html");
        assert!(idx.contains(r#"url=report.html"#));
        assert!(idx.contains(r#"href="report.html""#));
        // The real page still exists under its own name.
        assert!(text(&files, "report.html").contains("<h1>Report</h1>"));
    }

    #[test]
    fn assets_are_copied_under_their_relative_key() {
        let mut sp = space_with(&[("index", "<h1>Hi</h1>")]);
        sp.assets.insert(
            "assets/data.json".to_string(),
            Asset {
                content_type: "application/json; charset=utf-8",
                bytes: b"{\"ok\":true}".to_vec(),
            },
        );
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained);
        assert_eq!(
            find(&files, "assets/data.json").unwrap().bytes,
            b"{\"ok\":true}"
        );
    }

    #[test]
    fn write_files_lays_out_the_tree_on_disk() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static C: AtomicU32 = AtomicU32::new(0);
        let out = std::env::temp_dir().join(format!(
            "glasspad-build-{}-{}",
            std::process::id(),
            C.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&out);

        let mut sp = space_with(&[("index", "<h1>Hi</h1>")]);
        sp.assets.insert(
            "assets/x.txt".to_string(),
            Asset {
                content_type: "text/plain; charset=utf-8",
                bytes: b"hello".to_vec(),
            },
        );
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained);
        write_files(&out, &files).unwrap();

        // The referenced base lib actually exists at the path the page names,
        // relative to the page file — i.e. it resolves offline.
        assert!(out.join("index.html").is_file());
        assert!(out.join("_gp/v1/base.css").is_file());
        assert!(out.join("_gp/v1/bridge.js").is_file());
        assert_eq!(std::fs::read(out.join("assets/x.txt")).unwrap(), b"hello");

        let _ = std::fs::remove_dir_all(&out);
    }
}
