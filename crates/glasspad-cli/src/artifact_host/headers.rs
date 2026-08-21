//! HTTP adapter for the pure artifact-host header-policy decisions.

use axum::http::{HeaderName, HeaderValue, header};

pub use glasspad::artifact_host::headers::{
    PERMISSIONS_POLICY_DENY, artifact_csp_from_origins, self_origins, shell_csp,
};

/// Header list common to artifact + asset responses. Header-name/value types are
/// owned by the HTTP edge; the policy value itself lives in `glasspad-core`.
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
    fn hardening_headers_present() {
        let hs = hardening_headers();
        let names: Vec<_> = hs.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"x-content-type-options"));
        assert!(names.contains(&"referrer-policy"));
        assert!(names.contains(&"permissions-policy"));
    }
}
