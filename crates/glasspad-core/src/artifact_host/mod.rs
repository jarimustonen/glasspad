//! Pure rendering and security decisions for the HTML artifact host.
//!
//! HTTP, filesystem, and server adapters remain in `glasspad-cli`; this module
//! accepts already-loaded values and returns deterministic strings.

pub mod headers;
pub mod render;
pub mod sanitize;
pub mod shell;
pub mod wrap;

use sha2::{Digest, Sha256};

/// Stable content identity embedded in wrapped fragments and checked by the
/// submission edge. The first eight SHA-256 bytes are rendered as 16 hex chars.
pub fn content_version(body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    let mut out = String::with_capacity(16);
    for b in &digest[..8] {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{content_version, headers, shell, wrap};

    #[test]
    fn frozen_security_contract_is_testable_without_a_server() {
        let csp = headers::artifact_csp_from_origins("http://127.0.0.1:3000", true);
        assert!(csp.starts_with("sandbox allow-scripts allow-top-navigation-by-user-activation;"));
        assert!(csp.contains("connect-src 'none'"));
        assert!(!csp.contains("allow-same-origin"));

        let body = "<h1>hostile fragment</h1>";
        let wrapped = wrap::render_artifact(body, wrap::Theme::Auto);
        assert!(wrapped.contains(&format!(
            r#"<meta name="gp-content-version" content="{}">"#,
            content_version(body)
        )));
        assert!(wrapped.contains(r#"<script src="/_gp/v1/bridge.js" defer></script>"#));

        let nav = [("index", "\" sandbox=\"allow-scripts allow-same-origin")];
        let rendered =
            shell::render_with_groups("", "demo", "index", nav[0].1, &nav, &[], "nonce", None);
        assert!(
            rendered.contains(r#"sandbox="allow-scripts allow-top-navigation-by-user-activation""#)
        );
        assert!(!rendered.contains(r#"sandbox="allow-scripts allow-same-origin""#));
    }
}
