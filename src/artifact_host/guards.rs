//! Control-plane guards (design.md §5). The control/API surface never trusts the
//! sandbox, so these run independently of iframe isolation:
//!
//! * **Loopback bind** — enforced in `server::run` (`127.0.0.1`); binding a
//!   routable interface would require an explicit unsafe opt-in.
//! * **Host validation** (all routes) — defeats DNS rebinding: a hostile page
//!   that resolves its own name to `127.0.0.1` still sends *its* `Host` header,
//!   which we reject.
//! * **Origin rejection on the control API** — a sandboxed artifact (or any
//!   cross-origin page) carries `Origin: null` / a foreign origin on a state-
//!   changing request; the control API rejects both. Same-process CLI/curl send
//!   no `Origin` and are unaffected.

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Is this `Host` header one of the loopback names this server answers to?
/// Accepts host with or without the port (some agents omit it), `127.0.0.1`,
/// `localhost`, and IPv6 loopback. Anything else (an attacker's rebound name)
/// is rejected.
fn host_allowed(host: &str, port: u16) -> bool {
    let host = host.trim();
    // Strip a trailing :port if present (naive but fine for loopback names;
    // IPv6 literals are wrapped in [] so their inner colons are not split here).
    let (name, port_part) = if host.starts_with('[') {
        match host.rfind(']') {
            Some(end) => {
                let name = &host[..=end];
                let rest = &host[end + 1..];
                let p = rest.strip_prefix(':');
                (name, p)
            }
            None => (host, None),
        }
    } else {
        match host.rsplit_once(':') {
            Some((n, p)) => (n, Some(p)),
            None => (host, None),
        }
    };

    if let Some(p) = port_part {
        // If a port is present it must be ours — a foreign port is not us.
        if p.parse::<u16>() != Ok(port) {
            return false;
        }
    }
    matches!(name, "127.0.0.1" | "localhost" | "[::1]")
}

/// Global guard: reject requests whose `Host` header names something other than
/// this loopback server. A missing `Host` (non-browser tooling) is allowed —
/// DNS rebinding is a browser attack and always carries a `Host`.
pub async fn host_guard(State(port): State<u16>, req: Request, next: Next) -> Response {
    if let Some(host) = req.headers().get(header::HOST).and_then(|v| v.to_str().ok())
        && !host_allowed(host, port)
    {
        return (StatusCode::MISDIRECTED_REQUEST, "bad Host header").into_response();
    }
    next.run(req).await
}

/// Control-plane guard for the API: additionally reject a present `Origin` that
/// is `null` or not one of our loopback origins. Requests with no `Origin`
/// (CLI/curl) pass; browser cross-origin / sandboxed-frame requests do not.
pub async fn control_origin_guard(
    State(port): State<u16>,
    req: Request,
    next: Next,
) -> Response {
    if let Some(origin) = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok())
        && !origin_allowed(origin, port)
    {
        return (StatusCode::FORBIDDEN, "cross-origin request rejected").into_response();
    }
    next.run(req).await
}

fn origin_allowed(origin: &str, port: u16) -> bool {
    origin == format!("http://127.0.0.1:{port}") || origin == format!("http://localhost:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_accepts_loopback_names() {
        assert!(host_allowed("127.0.0.1:3000", 3000));
        assert!(host_allowed("localhost:3000", 3000));
        assert!(host_allowed("127.0.0.1", 3000));
        assert!(host_allowed("localhost", 3000));
        assert!(host_allowed("[::1]:3000", 3000));
    }

    #[test]
    fn host_rejects_rebinding_and_foreign_port() {
        assert!(!host_allowed("evil.example.com", 3000));
        assert!(!host_allowed("evil.example.com:3000", 3000));
        assert!(!host_allowed("127.0.0.1:9999", 3000)); // foreign port
        assert!(!host_allowed("attacker.local:3000", 3000));
    }

    #[test]
    fn origin_only_loopback() {
        assert!(origin_allowed("http://127.0.0.1:3000", 3000));
        assert!(origin_allowed("http://localhost:3000", 3000));
        assert!(!origin_allowed("null", 3000));
        assert!(!origin_allowed("http://evil.example.com", 3000));
        assert!(!origin_allowed("http://127.0.0.1:9999", 3000));
    }
}
