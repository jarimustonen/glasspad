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

/// The origin string the artifact CSP is allowed to name. Loopback only.
/// Both `127.0.0.1` and `localhost` resolve to the same server, but the CSP
/// names the numeric host the shell frames against, matching the iframe `src`.
pub fn self_origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
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
/// * `connect-src 'none'` — **the exfil boundary**: no `fetch`, `sendBeacon`,
///   `WebSocket`, or XHR to anywhere, including self. (Wave 2a widens this to the
///   named host for the SSE reload path; it never opens to a foreign host.)
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
    let host = self_origin(port);
    let eval = if allow_eval { " 'unsafe-eval'" } else { "" };
    format!(
        "sandbox allow-scripts allow-top-navigation-by-user-activation; \
         default-src 'none'; \
         script-src 'unsafe-inline'{eval} {host}; \
         style-src 'unsafe-inline' {host}; \
         img-src {host} data:; \
         font-src {host}; \
         connect-src 'none'; \
         object-src 'none'; \
         frame-src 'none'; \
         worker-src 'none'; \
         base-uri 'none'; \
         form-action 'none'; \
         frame-ancestors {host}"
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
    fn artifact_csp_names_explicit_host_not_self() {
        let csp = artifact_csp(3000, true);
        assert!(csp.contains("http://127.0.0.1:3000"));
        // 'self' is useless under a null origin; script/img/style must name host.
        assert!(csp.contains("script-src 'unsafe-inline' 'unsafe-eval' http://127.0.0.1:3000"));
    }

    #[test]
    fn artifact_csp_blocks_all_egress_channels() {
        let csp = artifact_csp(3000, true);
        assert!(csp.contains("connect-src 'none'")); // fetch/beacon/ws/xhr
        assert!(csp.contains("form-action 'none'")); // form POST exfil
        assert!(csp.contains("object-src 'none'"));
        assert!(csp.contains("base-uri 'none'"));
        // img-src must NOT be a wildcard — only host + data:
        assert!(csp.contains("img-src http://127.0.0.1:3000 data:"));
        assert!(!csp.contains("img-src *"));
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
