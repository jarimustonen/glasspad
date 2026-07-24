//! Security headers for the v0.2 HTML-artifact host (Wave 1 security gate).
//!
//! Two distinct trust zones, two distinct header sets:
//!
//! * **Artifact content** (`/{space}/_c/{slug}`) — hostile, agent-authored HTML
//!   run in a null-origin sandbox. Its response carries the `CSP: sandbox`
//!   directive (so a *direct open* is sandboxed too, not just the iframe) plus
//!   an **egress CSP that names the explicit host** — `'self'` is meaningless
//!   under a null origin, so exfiltration is bounded by enumerating the one host
//!   the artifact may talk to.
//! * **Trusted shell** (`/{space}/`, `/{space}/{slug}`) — first-party chrome
//!   that frames the artifact. Ordinary restrictive CSP + Trusted Types, plus a
//!   per-response nonce for its own inline bridge script.
//!
//! See `issues/html-artifact-host-rewrite/design.md` §3–§6.

use axum::http::{header, HeaderName, HeaderValue};

/// `Permissions-Policy` deny-list applied to every artifact and asset response.
/// Every powerful feature is denied to the (empty) allow-list `()`.
pub const PERMISSIONS_POLICY_DENY: &str = "accelerometer=(), autoplay=(), \
camera=(), clipboard-read=(), clipboard-write=(), cross-origin-isolated=(), \
display-capture=(), encrypted-media=(), fullscreen=(), gamepad=(), \
geolocation=(), gyroscope=(), hid=(), idle-detection=(), \
local-fonts=(), magnetometer=(), microphone=(), midi=(), payment=(), \
picture-in-picture=(), publickey-credentials-get=(), screen-wake-lock=(), \
serial=(), usb=(), web-share=(), xr-spatial-tracking=()";

/// The loopback origins the artifact CSP names. The shell is reachable at both
/// `http://127.0.0.1:PORT` and `http://localhost:PORT` (browsers treat them as
/// distinct origins), and the iframe `src` is relative, so it inherits whichever
/// one the user opened. The CSP therefore names **both** — otherwise opening the
/// shell over `localhost` would leave `frame-ancestors`/`script-src` naming only
/// `127.0.0.1`, and the browser would refuse to frame or run the artifact. Both
/// resolve to the same loopback server, so this widens nothing externally.
/// (`server::run` binds `127.0.0.1` only; `[::1]` is never actually served.)
pub fn self_origins(port: u16) -> String {
    format!("http://127.0.0.1:{port} http://localhost:{port}")
}

/// Build the artifact-content `Content-Security-Policy`.
///
/// * `sandbox allow-scripts` — sandboxes direct-opens (§3); scripts run but the
///   document has a **null origin** (no `allow-same-origin`), so it cannot read
///   the parent, app storage, or same-origin API responses.
/// * `default-src 'none'` — deny everything, then re-grant narrowly.
/// * `script-src` — inline (agents write inline) + `'unsafe-eval'` (Vega-Lite;
///   see design.md §4) + the explicit host so `/_gp/v1/*` base libs load via
///   classic `<script src>`.
/// * `connect-src` — **the exfil boundary**. Wave 1 set this to `'none'`; Wave 2a
///   widens it to name **exactly the loopback SSE-reload path** on both origins
///   (`http://127.0.0.1:PORT/_gp/reload http://localhost:PORT/_gp/reload`) and
///   nothing else. A CSP path-source is an exact-path match, so `fetch`/beacon/ws/
///   XHR to `/api/*`, any other path, any foreign host, or the network canary all
///   still violate the policy — the boundary stays closed except for live reload.
/// * `img-src` names the host + `data:` only — no external beacon pixels.
/// * `form-action 'none'`, `base-uri 'none'`, `object-src 'none'`,
///   `frame-src 'none'`, `worker-src 'none'` — close the remaining channels.
/// * `frame-ancestors` names the host so only our shell may frame it.
///
/// `allow_eval = false` produces the identical policy minus `'unsafe-eval'`.
/// That variant is **strictly tighter** and exists only so the adversarial suite
/// can empirically demonstrate that Vega-Lite-style `new Function(...)` requires
/// `'unsafe-eval'` (design.md §4). Production always serves `allow_eval = true`.
pub fn artifact_csp(port: u16, allow_eval: bool) -> String {
    let hosts = self_origins(port);
    let eval = if allow_eval { " 'unsafe-eval'" } else { "" };
    // The ONLY egress the artifact may open: the live-reload SSE endpoint, named
    // by exact loopback path on both origins. Everything else stays blocked.
    let reload = format!("http://127.0.0.1:{port}/_gp/reload http://localhost:{port}/_gp/reload");
    format!(
        "sandbox allow-scripts allow-top-navigation-by-user-activation; \
         default-src 'none'; \
         script-src 'unsafe-inline'{eval} {hosts}; \
         style-src 'unsafe-inline' {hosts}; \
         img-src {hosts} data:; \
         font-src {hosts}; \
         connect-src {reload}; \
         object-src 'none'; \
         frame-src 'none'; \
         worker-src 'none'; \
         base-uri 'none'; \
         form-action 'none'; \
         frame-ancestors {hosts}"
    )
}

/// Trusted-shell CSP. First-party chrome, so `'self'` is meaningful here.
/// A per-response `nonce` authorizes the shell's own inline bridge script — no
/// blanket `'unsafe-inline'`. Trusted Types is required so any accidental
/// `innerHTML` sink throws instead of executing artifact-derived text (§6).
pub fn shell_csp(nonce: &str) -> String {
    format!(
        "default-src 'self'; \
         script-src 'self' 'nonce-{nonce}'; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; \
         connect-src 'self'; \
         frame-src 'self'; \
         object-src 'none'; \
         base-uri 'none'; \
         form-action 'none'; \
         frame-ancestors 'none'; \
         require-trusted-types-for 'script'"
    )
}

/// Header list common to artifact + asset responses: `nosniff`, `no-referrer`,
/// and the `Permissions-Policy` deny-list. Returned as owned pairs so callers
/// can attach them to any response.
pub fn hardening_headers() -> Vec<(HeaderName, HeaderValue)> {
    vec![
        (
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
        (
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ),
        (
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static(PERMISSIONS_POLICY_DENY),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_csp_sandboxes_and_allows_scripts() {
        let csp = artifact_csp(3000, true);
        assert!(csp.starts_with("sandbox allow-scripts"));
        // No allow-same-origin — the null origin is the whole point.
        assert!(!csp.contains("allow-same-origin"));
    }

    #[test]
    fn artifact_csp_names_explicit_hosts_not_self() {
        let csp = artifact_csp(3000, true);
        // 'self' is useless under a null origin; script/img/style must name the
        // explicit loopback origins — BOTH spellings the shell is reachable at.
        assert!(csp.contains("http://127.0.0.1:3000"));
        assert!(csp.contains("http://localhost:3000"));
        assert!(csp.contains("script-src 'unsafe-inline' 'unsafe-eval' http://127.0.0.1:3000 http://localhost:3000"));
        assert!(csp.contains("frame-ancestors http://127.0.0.1:3000 http://localhost:3000"));
        assert!(!csp.contains("'self'"));
    }

    #[test]
    fn artifact_csp_blocks_all_egress_channels() {
        let csp = artifact_csp(3000, true);
        // connect-src is scoped to exactly the loopback SSE-reload path — no
        // wildcard, no bare host, no foreign origin. `/api/*` and canaries stay
        // blocked (path-source is an exact match), so the exfil boundary holds.
        assert!(csp.contains(
            "connect-src http://127.0.0.1:3000/_gp/reload http://localhost:3000/_gp/reload"
        ));
        assert!(!csp.contains("connect-src *"));
        assert!(!csp.contains("connect-src http://127.0.0.1:3000 ")); // not the bare origin
        assert!(csp.contains("form-action 'none'")); // form POST exfil
        assert!(csp.contains("object-src 'none'"));
        assert!(csp.contains("base-uri 'none'"));
        // img-src must NOT be a wildcard — only the named hosts + data:
        assert!(csp.contains("img-src http://127.0.0.1:3000 http://localhost:3000 data:"));
        assert!(!csp.contains("img-src *"));
    }

    #[test]
    fn artifact_csp_connect_src_does_not_grant_api_or_foreign() {
        let csp = artifact_csp(3000, true);
        // The reload path is the whole connect-src allowance; a bare origin would
        // (wrongly) permit `/api/*`. Assert the source always carries the path.
        assert!(csp.contains("/_gp/reload"));
        assert!(!csp.contains("connect-src 'self'"));
        assert!(!csp.contains("gp-exfil.invalid"));
    }

    #[test]
    fn artifact_csp_eval_toggle() {
        assert!(artifact_csp(3000, true).contains("'unsafe-eval'"));
        assert!(!artifact_csp(3000, false).contains("'unsafe-eval'"));
    }

    #[test]
    fn artifact_csp_port_is_reflected() {
        assert!(artifact_csp(8123, true).contains("http://127.0.0.1:8123"));
        assert!(!artifact_csp(8123, true).contains(":3000"));
    }

    #[test]
    fn shell_csp_uses_nonce_and_trusted_types() {
        let csp = shell_csp("abc123");
        // Script is nonce-gated — no 'unsafe-inline' in the script-src directive.
        assert!(csp.contains("script-src 'self' 'nonce-abc123'"));
        assert!(!csp.contains("script-src 'self' 'unsafe-inline'"));
        assert!(csp.contains("require-trusted-types-for 'script'"));
        assert!(csp.contains("frame-ancestors 'none'"));
    }

    #[test]
    fn hardening_headers_present() {
        let hs = hardening_headers();
        let names: Vec<_> = hs.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"x-content-type-options"));
        assert!(names.contains(&"referrer-policy"));
        assert!(names.contains(&"permissions-policy"));
    }
}
