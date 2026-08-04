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
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Is this `Host` header one of the loopback names this server answers to?
/// The server binds `127.0.0.1` only, so the sole valid hosts are `127.0.0.1`
/// and `localhost`, optionally with our port. Parsing is strict: the name must
/// match *exactly* (no trailing garbage), and a present port must be ours.
/// Anything else — a rebound attacker name, a foreign port, IPv6, trailing
/// junk like `[::1]evil` — is rejected. **Fails closed.**
fn host_allowed(host: &str, port: u16) -> bool {
    let (name, port_part) = match host.split_once(':') {
        Some((n, p)) => (n, Some(p)),
        None => (host, None),
    };
    // A present port must parse exactly to ours (rejects `:3000evil`, `:9999`).
    if let Some(p) = port_part
        && p.parse::<u16>() != Ok(port)
    {
        return false;
    }
    matches!(name, "127.0.0.1" | "localhost")
}

/// Global guard: reject requests whose `Host` header names something other than
/// this loopback server, **and** requests that omit or mangle it. A security
/// allowlist fails closed: browsers always send a `Host`/`:authority`, and the
/// CLI's `reqwest`/`curl` set it automatically — so nothing legitimate is lost.
pub async fn host_guard(State(port): State<u16>, req: Request, next: Next) -> Response {
    match req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
    {
        Some(host) if host_allowed(host, port) => next.run(req).await,
        Some(_) => (StatusCode::MISDIRECTED_REQUEST, "bad Host header").into_response(),
        None => (StatusCode::BAD_REQUEST, "missing or invalid Host header").into_response(),
    }
}

/// Control-plane guard for the API: additionally reject a present `Origin` that
/// is `null` or not one of our loopback origins. Requests with no `Origin`
/// (CLI/curl) pass; browser cross-origin / sandboxed-frame requests do not.
///
/// **Currently unwired (Wave 3, decision D2):** its sole consumer was the v0.1
/// `/api/pads` control surface, removed with the legacy routes. It is retained
/// intact — not deleted — because it is the tested `Origin`-rejection primitive
/// the Wave 3a artifact-host control/mutation surface will re-attach. `#[allow]`
/// keeps it warning-free until then; do not weaken its logic.
#[allow(dead_code)]
pub async fn control_origin_guard(State(port): State<u16>, req: Request, next: Next) -> Response {
    if let Some(origin) = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        && !origin_allowed(origin, port)
    {
        return (StatusCode::FORBIDDEN, "cross-origin request rejected").into_response();
    }
    next.run(req).await
}

// Called only by `control_origin_guard` (unwired since Wave 3) and its unit
// test; retained alongside the guard for the Wave 3a control surface.
#[allow(dead_code)]
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
    }

    #[test]
    fn host_rejects_rebinding_foreign_port_and_malformed() {
        assert!(!host_allowed("evil.example.com", 3000));
        assert!(!host_allowed("evil.example.com:3000", 3000));
        assert!(!host_allowed("127.0.0.1:9999", 3000)); // foreign port
        assert!(!host_allowed("attacker.local:3000", 3000));
        // Strict parsing: no trailing garbage, no IPv6, no junk port.
        assert!(!host_allowed("[::1]:3000", 3000)); // IPv6 not served (v4 bind)
        assert!(!host_allowed("[::1]evil", 3000));
        assert!(!host_allowed("127.0.0.1:3000evil", 3000));
        assert!(!host_allowed("localhost.evil.com", 3000));
        assert!(!host_allowed("", 3000));
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
