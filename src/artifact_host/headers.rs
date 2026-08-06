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

use axum::http::{HeaderName, HeaderValue, header};

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

/// Build the artifact-content `Content-Security-Policy`, parameterized by the
/// exact origin(s) it names.
///
/// * `sandbox allow-scripts` — sandboxes direct-opens (§3); scripts run but the
///   document has a **null origin** (no `allow-same-origin`), so it cannot read
///   the parent, app storage, or same-origin API responses.
/// * `default-src 'none'` — deny everything, then re-grant narrowly.
/// * `script-src` — inline (agents write inline) + `'unsafe-eval'` (Vega-Lite;
///   see design.md §4) + the explicit host so `/_gp/v1/*` base libs load via
///   classic `<script src>`.
/// * `connect-src 'none'` — **the exfil boundary**: no `fetch`, `sendBeacon`,
///   `WebSocket`, or XHR to anywhere, including self. Live reload is driven from
///   the **trusted shell** (its `connect-src 'self'` permits the `EventSource`),
///   so the artifact needs no connect authority and this stays fully closed.
/// * `img-src` names the host + `data:` only — no external beacon pixels.
/// * `form-action 'none'`, `base-uri 'none'`, `object-src 'none'`,
///   `frame-src 'none'`, `worker-src 'none'` — close the remaining channels.
/// * `frame-ancestors` names the host so only our shell may frame it.
///
/// `allow_eval = false` produces the identical policy minus `'unsafe-eval'`
/// (strictly tighter; the adversarial suite's Vega `new Function` probe). Production
/// serves `allow_eval = true`.
///
/// `origins` is the exact space-separated origin list the artifact may load its
/// `/_gp/v1/*` script/style from and be framed by — the loopback path passes both
/// loopback spellings ([`self_origins`]); the hosted run mode passes its single
/// public origin. **Only the named host changes**: the null-origin `sandbox`, the
/// `connect-src 'none'` boundary, and every other closure are identical in both
/// modes, so the hosted mode reuses — never widens — the frozen boundary. `'self'`
/// is meaningless under a null origin, which is why the origin is named explicitly.
pub fn artifact_csp_from_origins(origins: &str, allow_eval: bool) -> String {
    let hosts = origins;
    let eval = if allow_eval { " 'unsafe-eval'" } else { "" };
    format!(
        "sandbox allow-scripts allow-top-navigation-by-user-activation; \
         default-src 'none'; \
         script-src 'unsafe-inline'{eval} {hosts}; \
         style-src 'unsafe-inline' {hosts}; \
         img-src {hosts} data:; \
         font-src {hosts}; \
         connect-src 'none'; \
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
/// blanket `'unsafe-inline'`. **`script-src` names only the nonce, not `'self'`**:
/// the shell loads no same-origin script *file* (its only script is the inline,
/// nonce-authorized one), and `'self'` would otherwise authorize a parser-created
/// `<script src="/{space}/assets/attacker.js">` — agent-authored content on the
/// same origin — if any markup injection into the shell ever appeared. Dropping it
/// is strictly tighter and shrinks the injection blast radius (Trusted Types does
/// not stop parser-created `<script src>`). Trusted Types is required so any
/// accidental `innerHTML` sink throws instead of executing artifact-derived
/// text (§6); with no default policy defined, that throw is active today.
pub fn shell_csp(nonce: &str) -> String {
    format!(
        "default-src 'self'; \
         script-src 'nonce-{nonce}'; \
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
        let csp = artifact_csp_from_origins(&self_origins(3000), true);
        assert!(csp.starts_with("sandbox allow-scripts"));
        // No allow-same-origin — the null origin is the whole point.
        assert!(!csp.contains("allow-same-origin"));
    }

    #[test]
    fn artifact_csp_names_explicit_hosts_not_self() {
        let csp = artifact_csp_from_origins(&self_origins(3000), true);
        // 'self' is useless under a null origin; script/img/style must name the
        // explicit loopback origins — BOTH spellings the shell is reachable at.
        assert!(csp.contains("http://127.0.0.1:3000"));
        assert!(csp.contains("http://localhost:3000"));
        assert!(csp.contains(
            "script-src 'unsafe-inline' 'unsafe-eval' http://127.0.0.1:3000 http://localhost:3000"
        ));
        assert!(csp.contains("frame-ancestors http://127.0.0.1:3000 http://localhost:3000"));
        assert!(!csp.contains("'self'"));
    }

    #[test]
    fn artifact_csp_blocks_all_egress_channels() {
        let csp = artifact_csp_from_origins(&self_origins(3000), true);
        assert!(csp.contains("connect-src 'none'")); // fetch/beacon/ws/xhr, incl. self
        assert!(csp.contains("form-action 'none'")); // form POST exfil
        assert!(csp.contains("object-src 'none'"));
        assert!(csp.contains("base-uri 'none'"));
        // img-src must NOT be a wildcard — only the named hosts + data:
        assert!(csp.contains("img-src http://127.0.0.1:3000 http://localhost:3000 data:"));
        assert!(!csp.contains("img-src *"));
        // No path-scoped SSE source leaked into the artifact policy — reload is
        // shell-side only, so the artifact stays fully closed.
        assert!(!csp.contains("/_gp/reload"));
    }

    #[test]
    fn artifact_csp_eval_toggle() {
        assert!(artifact_csp_from_origins(&self_origins(3000), true).contains("'unsafe-eval'"));
        assert!(!artifact_csp_from_origins(&self_origins(3000), false).contains("'unsafe-eval'"));
    }

    #[test]
    fn artifact_csp_port_is_reflected() {
        assert!(
            artifact_csp_from_origins(&self_origins(8123), true).contains("http://127.0.0.1:8123")
        );
        assert!(!artifact_csp_from_origins(&self_origins(8123), true).contains(":3000"));
    }

    #[test]
    fn shell_csp_uses_nonce_and_trusted_types() {
        let csp = shell_csp("abc123");
        // Script is nonce-gated — no 'unsafe-inline', and NO 'self' either (the
        // shell loads no same-origin script file; 'self' would authorize an
        // agent-authored /{space}/assets/*.js if markup injection ever appeared).
        assert!(csp.contains("script-src 'nonce-abc123'"));
        assert!(!csp.contains("script-src 'self'"));
        assert!(!csp.contains("'unsafe-inline' 'nonce"));
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
