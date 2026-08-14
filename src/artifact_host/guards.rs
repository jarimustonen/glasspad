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

/// The DNS-rebinding Host allowlist for a running server: our port, plus (only in
/// `loopback serve --bind` LAN mode, [`crate::server::LanExposure`]) the ONE extra
/// non-loopback host the operator explicitly opted the server into. The two
/// loopback names are always accepted; `allow_host` is `None` in the default
/// loopback-only mode, so the guard is byte-compatible with the pre-LAN behavior.
#[derive(Clone, Debug)]
pub struct HostPolicy {
    /// The TCP port a valid `Host` must name (or omit).
    pub port: u16,
    /// The one extra allowlisted host (lowercased, no port), from `--bind`. `None`
    /// keeps the guard loopback-only (default).
    pub allow_host: Option<String>,
}

impl HostPolicy {
    /// The default loopback-only policy (no LAN host) — byte-compatible with the
    /// pre-LAN guard.
    pub fn loopback(port: u16) -> Self {
        Self {
            port,
            allow_host: None,
        }
    }
}

/// Is this `Host` header one this server answers to? Always `127.0.0.1` /
/// `localhost`; in LAN mode also the single `policy.allow_host` the operator named
/// via `--bind`. Parsing is strict: the name must match *exactly* (no trailing
/// garbage), and a present port must be ours. Anything else — a rebound attacker
/// name, a foreign IP, a foreign port, IPv6, trailing junk like `[::1]evil` — is
/// rejected. **Fails closed** (the DNS-rebinding defense is an allowlist).
fn host_allowed(host: &str, policy: &HostPolicy) -> bool {
    let (name, port_part) = match host.split_once(':') {
        Some((n, p)) => (n, Some(p)),
        None => (host, None),
    };
    // A present port must parse exactly to ours (rejects `:3000evil`, `:9999`).
    if let Some(p) = port_part
        && p.parse::<u16>() != Ok(policy.port)
    {
        return false;
    }
    if matches!(name, "127.0.0.1" | "localhost") {
        return true;
    }
    // The one explicitly-configured LAN host, matched case-insensitively (hostnames
    // are case-insensitive; `allow_host` is already lowercased). An IP compares the
    // same either way. Still a strict allowlist — one exact host, nothing wildcarded.
    match &policy.allow_host {
        Some(h) => name.eq_ignore_ascii_case(h),
        None => false,
    }
}

/// Global guard: reject requests whose `Host` header names something other than
/// this server's allowlist ([`HostPolicy`]), **and** requests that omit or mangle
/// it. A security allowlist fails closed: browsers always send a `Host`/
/// `:authority`, and the CLI's `reqwest`/`curl` set it automatically — so nothing
/// legitimate is lost. In LAN mode the allowlist gains exactly the one opted-in
/// host; a DNS-rebinding attacker's foreign `Host` is still refused.
pub async fn host_guard(State(policy): State<HostPolicy>, req: Request, next: Next) -> Response {
    match req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
    {
        Some(host) if host_allowed(host, &policy) => next.run(req).await,
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
        let p = HostPolicy::loopback(3000);
        assert!(host_allowed("127.0.0.1:3000", &p));
        assert!(host_allowed("localhost:3000", &p));
        assert!(host_allowed("127.0.0.1", &p));
        assert!(host_allowed("localhost", &p));
    }

    #[test]
    fn host_rejects_rebinding_foreign_port_and_malformed() {
        let p = HostPolicy::loopback(3000);
        assert!(!host_allowed("evil.example.com", &p));
        assert!(!host_allowed("evil.example.com:3000", &p));
        assert!(!host_allowed("127.0.0.1:9999", &p)); // foreign port
        assert!(!host_allowed("attacker.local:3000", &p));
        // Strict parsing: no trailing garbage, no IPv6, no junk port.
        assert!(!host_allowed("[::1]:3000", &p)); // IPv6 not served (v4 bind)
        assert!(!host_allowed("[::1]evil", &p));
        assert!(!host_allowed("127.0.0.1:3000evil", &p));
        assert!(!host_allowed("localhost.evil.com", &p));
        assert!(!host_allowed("", &p));
    }

    #[test]
    fn lan_host_allowlist_accepts_only_the_configured_host() {
        // LAN mode (`loopback serve --bind 192.168.1.50`): loopback still works, the
        // one opted-in host is accepted (case-insensitively, exact port), and every
        // OTHER host — a DNS-rebinding attacker, a different LAN IP, a foreign port —
        // is still refused. The allowlist stays a strict, fail-closed enumeration.
        let p = HostPolicy {
            port: 3000,
            allow_host: Some("192.168.1.50".into()),
        };
        assert!(host_allowed("192.168.1.50:3000", &p));
        assert!(host_allowed("192.168.1.50", &p));
        assert!(host_allowed("127.0.0.1:3000", &p)); // loopback still ok
        assert!(host_allowed("localhost", &p));
        // DNS-rebinding / foreign hosts still rejected.
        assert!(!host_allowed("attacker.example.com:3000", &p));
        assert!(!host_allowed("192.168.1.99:3000", &p)); // a different LAN IP
        assert!(!host_allowed("192.168.1.50:9999", &p)); // foreign port
        assert!(!host_allowed("192.168.1.50.evil.com:3000", &p));

        // A hostname bind matches case-insensitively but nothing else.
        let ph = HostPolicy {
            port: 8080,
            allow_host: Some("mymac.local".into()),
        };
        assert!(host_allowed("MyMac.local:8080", &ph));
        assert!(host_allowed("mymac.local", &ph));
        assert!(!host_allowed("mymac.local:3000", &ph)); // foreign port
        assert!(!host_allowed("evil.mymac.local:8080", &ph));
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
