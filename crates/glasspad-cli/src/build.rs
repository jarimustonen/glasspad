//! `glasspad build` — static, self-contained render of a space to HTML files.
//!
//! A space directory is scanned by the **same security-checked scanner** the
//! server uses (`space::scan_dir` via `server::scan_named`: symlink / traversal /
//! reserved-slug / collision / size are all rejected before a single byte is
//! written), then each artifact is wrapped through the **same render seam** the
//! content route uses (`wrap::render_artifact`): a fragment is wrapped into a
//! themed document with `base.css` linked + `bridge.js` injected; a full document
//! is emitted verbatim. The renderer is **not** forked — each page is byte-for-byte
//! the **content-route** document the server would serve at `/{space}/_c/{slug}`
//! (modulo the self-contained base-lib path localization).
//!
//! **What the build does NOT reproduce.** The live host frames that content in a
//! **trusted parent shell** (nav chrome + a null-origin sandboxed iframe) and sets a
//! per-response CSP; a static file carries no HTTP headers and there is no shell. So
//! build output (a) runs **unsandboxed** as a top-level document — an artifact's own
//! script runs with the deploy origin's authority, so build only spaces you trust;
//! and (b) has **no cross-artifact nav** — `bridge.js` (injected into fragments) is
//! inert without a parent shell, and an author's extensionless same-space link
//! (`href="other-slug"`) does not resolve to `other-slug.html`.
//!
//! These are surfaced as build `warnings[]` (see `cli::build`). The output is meant
//! for an **offline docsite / external preview transport** — the input-side
//! guarantees (the scanner refusing hostile *filesystem* structure) carry over, but
//! it does NOT sanitize artifact HTML and does NOT reproduce the response-side
//! sandbox/CSP. A cleaner future path is to also emit the trusted shell for full
//! nav fidelity (flagged as a follow-up).
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
///
/// `favicon` is the validated repo emoji (`.glasspad.yaml` `favicon:`; `None` → the
/// built-in default). Because a static build has **no trusted shell**, each built
/// page IS its own outer document, so the favicon `<link>` is injected into the
/// first-party `<head>` of a **fragment-wrapped** page (the head glasspad emits). A
/// full document is emitted verbatim — its author owns the whole page, including its
/// `<head>`, so it is never rewritten (mirrors the base-lib localization policy).
pub fn wrapped_page(artifact_html: &str, mode: LibMode, favicon: Option<&str>) -> String {
    let out = wrap::render_artifact(artifact_html, Theme::Auto);
    if wrap::is_fragment(artifact_html) {
        let out = inject_favicon(out, favicon);
        if mode == LibMode::SelfContained {
            localize_base_libs(out)
        } else {
            out
        }
    } else {
        out
    }
}

/// Inject the emoji favicon `<link>` into a fragment-wrapped page's first-party
/// `<head>` (right before `</head>`). Scoped to the head `wrap::wrap_fragment`
/// emits, so the untrusted fragment body (after `</head>`) is never touched. Only
/// called for fragments — a full document is emitted verbatim. `link_tag`
/// base64-encodes the whole SVG, so the injected attribute carries no markup.
fn inject_favicon(wrapped: String, favicon: Option<&str>) -> String {
    let link = crate::favicon::link_tag(favicon);
    match wrapped.find("</head>") {
        Some(i) => {
            let mut out = String::with_capacity(wrapped.len() + link.len() + 1);
            out.push_str(&wrapped[..i]);
            out.push_str(&link);
            out.push('\n');
            out.push_str(&wrapped[i..]);
            out
        }
        // wrap_fragment always emits a </head>; if that ever changes, fail loud in
        // debug/tests (so the invariant is caught) while release stays graceful — a
        // missing favicon is never worth aborting a build over.
        None => {
            debug_assert!(
                false,
                "inject_favicon: wrap output has no </head> — the fragment-wrap invariant broke"
            );
            wrapped
        }
    }
}

/// Rewrite the two base-lib refs `wrap::wrap_fragment` injects from the absolute
/// server path (`/_gp/v1/…`) to a **relative** one (`_gp/v1/…`). The rewrite is
/// **scoped to the `<head>` region** `wrap` emits — everything before the first
/// `</head>` — where the fragment body (untrusted author bytes, inserted after
/// `</head>`) can never appear. So an author who literally writes
/// `href="/_gp/v1/base.css"` inside their fragment body is left untouched; only
/// `wrap`'s own injected head tags are localized.
///
/// The match is still an exact string against `wrap`'s emitted tags; the build's
/// tests assert the rewrite actually fired (and that a body occurrence is *not*
/// rewritten), so a change to `wrap`'s tag formatting fails loudly rather than
/// silently leaving an unresolvable path. (An ideal seam would parameterize the
/// base path in `wrap` itself — flagged as a follow-up; `wrap` is the frozen
/// security seam, so this head-scoped post-process avoids touching it.)
fn localize_base_libs(wrapped: String) -> String {
    // Split at the end of the injected head so only first-party scaffold bytes are
    // rewritten; the fragment body after `</head>` is passed through verbatim.
    let split = wrapped
        .find("</head>")
        .map(|i| i + "</head>".len())
        .unwrap_or(wrapped.len());
    let (head, body) = wrapped.split_at(split);
    let head = head
        .replace("href=\"/_gp/v1/base.css\"", "href=\"_gp/v1/base.css\"")
        .replace("src=\"/_gp/v1/bridge.js\"", "src=\"_gp/v1/bridge.js\"");
    head + body
}

/// A minimal `index.html` that redirects to `home` (used when the space's home
/// slug is not literally `index`, so the output still has a canonical entry point).
/// `home` is a validated slug (`[a-z0-9][a-z0-9-]*`), so it needs no escaping — the
/// grammar excludes every HTML/URL metacharacter. It carries the favicon too (it is a
/// built outer document a visitor may briefly see), via the same `link_tag` seam.
fn index_redirect(home: &str, favicon: Option<&str>) -> String {
    let favicon_link = crate::favicon::link_tag(favicon);
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n\
         {favicon_link}\n\
         <meta http-equiv=\"refresh\" content=\"0; url={home}.html\">\n\
         <title>Redirecting…</title>\n</head><body>\n\
         <p><a href=\"{home}.html\">Continue to {home}</a></p>\n</body></html>\n",
        favicon_link = favicon_link,
        home = home
    )
}

/// Plan every file a build of `space` emits, in deterministic order. Pure — it
/// allocates the output bytes but touches no filesystem, so it is exhaustively
/// unit-testable. The caller ([`write_files`]) is the only thing that does I/O.
///
/// `favicon` is the validated repo emoji threaded into each fragment page's outer
/// `<head>` (see [`wrapped_page`]); `None` uses the built-in default.
///
/// Emits, in order: one `<slug>.html` per artifact (wrapped through
/// [`wrapped_page`]); an `index.html` **redirect** to the home artifact when the
/// home slug is not itself `index` (when it is, its own `index.html` is the entry
/// point); every static asset under its space-relative `assets/…` key; and — in
/// [`LibMode::SelfContained`] — the pinned base libs under `_gp/v1/…`.
pub fn plan(
    space: &Space,
    home: Option<&str>,
    mode: LibMode,
    favicon: Option<&str>,
) -> Vec<OutFile> {
    let mut files = Vec::new();

    // One wrapped page per artifact (BTreeMap → deterministic slug order).
    for (slug, artifact) in &space.artifacts {
        files.push(OutFile {
            rel_path: format!("{slug}.html"),
            bytes: wrapped_page(&artifact.html, mode, favicon).into_bytes(),
        });
    }

    // A canonical `index.html` entry point. The scanner resolves `home` as
    // `index` > `home` > first-in-nav, so `home == "index"` **iff** an `index`
    // artifact exists — in which case that artifact's own page already is
    // `index.html` (no redirect). For any other home we emit a redirect to it;
    // `home != "index"` therefore guarantees no `index` artifact and thus no
    // collision at `index.html` (and no self-redirect loop).
    if let Some(home) = home
        && home != "index"
    {
        debug_assert!(
            space.artifacts.contains_key(home),
            "resolved home {home:?} must be a real artifact slug"
        );
        files.push(OutFile {
            rel_path: "index.html".to_string(),
            bytes: index_redirect(home, favicon).into_bytes(),
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
    // no running host. `gp_asset` is the single source of truth for their bytes; a
    // `BASE_LIB_NAMES` entry that fails to resolve is a build-integrity bug (the two
    // lists must agree), so fail loud rather than silently omit a lib the report
    // then claims was bundled.
    if mode == LibMode::SelfContained {
        for name in fixtures::BASE_LIB_NAMES {
            let (_, body) = fixtures::gp_asset(name)
                .unwrap_or_else(|| panic!("BASE_LIB_NAMES entry {name:?} has no gp_asset body"));
            files.push(OutFile {
                rel_path: format!("_gp/v1/{name}"),
                bytes: body.as_bytes().to_vec(),
            });
        }
    }

    files
}

/// Write a planned file set under `out`, creating parent directories as needed.
/// `out` is created if absent. Each `rel_path` is a build-produced relative,
/// `/`-separated path (see [`OutFile`]); it is nonetheless **re-validated here**
/// segment by segment — empty, `.`, `..`, absolute, and backslash-bearing segments
/// are rejected — so this public entry point can never write outside `out`, even if
/// a future caller (or a scanner regression that leaks a `..` asset key) hands it a
/// malformed path. Each validated segment is joined with the platform separator so
/// the relative path maps to the right nested location on any platform.
pub fn write_files(out: &Path, files: &[OutFile]) -> std::io::Result<()> {
    std::fs::create_dir_all(out)?;
    for f in files {
        let mut path = out.to_path_buf();
        for seg in f.rel_path.split('/') {
            if seg.is_empty() || seg == "." || seg == ".." || seg.contains('\\') {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("refusing unsafe output path segment in {:?}", f.rel_path),
                ));
            }
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
        let page = wrapped_page("<h1>Hi</h1>", LibMode::SelfContained, None);
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
        assert_eq!(wrapped_page(full, LibMode::SelfContained, None), full);
        assert_eq!(wrapped_page(full, LibMode::SharedLibs, None), full);
    }

    #[test]
    fn fragment_build_page_carries_emoji_favicon_in_head() {
        // The built page IS its own outer document (no shell), so the favicon <link>
        // is injected into the fragment's first-party <head>, before </head> and
        // before the body — with the configured emoji base64'd into the SVG.
        let page = wrapped_page("<h1>Hi</h1>", LibMode::SelfContained, Some("🚀"));
        let link = page
            .find(r#"<link rel="icon" type="image/svg+xml" href="data:image/svg+xml;base64,"#)
            .expect("favicon link injected");
        assert!(link < page.find("</head>").unwrap());
        assert!(link < page.find("<h1>Hi</h1>").unwrap());
        use base64::Engine as _;
        let b64 = page[link..]
            .split("base64,")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let svg = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .unwrap(),
        )
        .unwrap();
        assert!(svg.contains('🚀'));
        // No configured emoji → the default favicon is still present on every page.
        let dflt = wrapped_page("<h1>Hi</h1>", LibMode::SelfContained, None);
        assert!(dflt.contains(r#"<link rel="icon" type="image/svg+xml""#));
    }

    #[test]
    fn full_document_build_page_gets_no_injected_favicon() {
        // A full document is the author's own outer document — verbatim, so build must
        // NOT inject a favicon into it (byte-for-byte unchanged even with one configured).
        let full = "<!doctype html><html><head><title>x</title></head><body>x</body></html>";
        assert_eq!(wrapped_page(full, LibMode::SelfContained, Some("🚀")), full);
        assert_eq!(wrapped_page(full, LibMode::SharedLibs, Some("🚀")), full);
    }

    #[test]
    fn self_contained_localizes_fragment_base_libs_to_relative() {
        let page = wrapped_page("<h1>Hi</h1>", LibMode::SelfContained, None);
        // The wrap-injected refs are rewritten to a relative path…
        assert!(page.contains(r#"href="_gp/v1/base.css""#));
        assert!(page.contains(r#"src="_gp/v1/bridge.js""#));
        // …and the absolute server path is gone (so file:// resolves them).
        assert!(!page.contains(r#"href="/_gp/v1/base.css""#));
        assert!(!page.contains(r#"src="/_gp/v1/bridge.js""#));
    }

    #[test]
    fn localize_does_not_touch_author_body_bytes() {
        // A fragment whose BODY literally contains the injected-tag strings must be
        // left untouched: only the first-party `<head>` scaffold is localized. This
        // proves the "can never touch an author's absolute ref" claim.
        let fragment = "<h1>Docs</h1><p>Load it with <code>href=\"/_gp/v1/base.css\"</code></p>\
             <script src=\"/_gp/v1/bridge.js\"></script>";
        let page = wrapped_page(fragment, LibMode::SelfContained, None);
        // The head's injected refs are localized…
        let head = &page[..page.find("</head>").unwrap()];
        assert!(head.contains(r#"href="_gp/v1/base.css""#));
        assert!(head.contains(r#"src="_gp/v1/bridge.js""#));
        // …but the author's body copies of the same strings survive verbatim.
        let body = &page[page.find("</head>").unwrap()..];
        assert!(body.contains(r#"href="/_gp/v1/base.css""#));
        assert!(body.contains(r#"src="/_gp/v1/bridge.js""#));
    }

    #[test]
    fn write_files_refuses_unsafe_path_segments() {
        let out =
            std::env::temp_dir().join(format!("glasspad-build-unsafe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        for bad in ["../escape.html", "a//b.html", "./x.html", "a/../../x"] {
            let files = vec![OutFile {
                rel_path: bad.to_string(),
                bytes: b"x".to_vec(),
            }];
            let err = write_files(&out, &files).expect_err("must reject unsafe path");
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        }
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn shared_libs_keeps_absolute_refs_and_omits_libs() {
        let page = wrapped_page("<h1>Hi</h1>", LibMode::SharedLibs, None);
        // Absolute server path is kept (resolved by whatever serves the root).
        assert!(page.contains(r#"href="/_gp/v1/base.css""#));
        assert!(page.contains(r#"src="/_gp/v1/bridge.js""#));

        let sp = space_with(&[("index", "<h1>Hi</h1>")]);
        let files = plan(&sp, sp.home.as_deref(), LibMode::SharedLibs, None);
        // No base libs are bundled in shared-libs mode.
        assert!(!files.iter().any(|f| f.rel_path.starts_with("_gp/v1/")));
    }

    #[test]
    fn self_contained_bundles_libs_that_pages_actually_reference() {
        // The "resolves its base libs offline" contract: a self-contained fragment
        // page references `_gp/v1/base.css` + `_gp/v1/bridge.js`, and both of those
        // exact relative paths are present as real, non-empty files in the plan.
        let sp = space_with(&[("index", "<h1>Hi</h1>")]);
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained, None);
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
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained, None);
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
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained, None);
        let idx = text(&files, "index.html");
        assert!(idx.contains(r#"url=report.html"#));
        assert!(idx.contains(r#"href="report.html""#));
        // The real page still exists under its own name.
        assert!(text(&files, "report.html").contains("<h1>Report</h1>"));
    }

    #[test]
    fn non_index_redirect_page_carries_the_favicon() {
        // The synthetic index.html redirect is a built outer document a visitor may
        // briefly see, so it too gets the favicon link.
        let sp = space_with(&[("report", "<h1>Report</h1>")]);
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained, Some("🚀"));
        let idx = text(&files, "index.html");
        let link = idx
            .find(r#"<link rel="icon" type="image/svg+xml" href="data:image/svg+xml;base64,"#)
            .expect("redirect page carries the favicon");
        assert!(link < idx.find("</head>").unwrap());
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
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained, None);
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
        let files = plan(&sp, sp.home.as_deref(), LibMode::SelfContained, None);
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
