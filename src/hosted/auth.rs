//! Ingest authentication for the hosted share server (the write surface).
//!
//! The read surface is public-by-design (capability URLs); the **write** surface
//! is API-key-authenticated. An operator-provided key file (`--api-key-file`) maps
//! bearer tokens to tenant ids; a `POST /api/v1/pages` must carry
//! `Authorization: Bearer <key>`. Verification is **fail-closed** and
//! **constant-time** (see [`KeyTable::authenticate`]): a missing/empty/malformed
//! token, or any token not in the table, is rejected with `401` and no key detail.
//!
//! The key file is loaded once at startup into an immutable table. A missing,
//! empty, or malformed file is a hard startup error — the server never comes up
//! with an ingest surface that authenticates nobody (accidental lockout is better
//! than silent open) or everybody.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::artifact_host::valid_space;
use glasspad::security::ct_eq;

/// Minimum accepted API-key length. Operator keys should be high-entropy; we
/// refuse trivially-weak (short) keys at load so a fat-fingered config can't stand
/// up a guessable ingest surface. Not a substitute for the operator generating
/// random keys — a floor, not a ceiling.
pub const MIN_API_KEY_LEN: usize = 32;

/// One authenticated tenant, resolved from a matched API key. Carried in the
/// request extensions by [`ingest_auth`] so the ingest handler attributes the page
/// to the authenticated tenant — never to client-supplied data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tenant(pub String);

/// Immutable table of `(tenant, key)` pairs loaded from the operator key file.
pub struct KeyTable {
    entries: Vec<(String, String)>,
}

/// Everything that can go wrong loading the key file. Each renders an informative,
/// line-numbered message (AI-first CLI contract) so the operator can fix the file.
#[derive(Debug)]
pub enum KeyFileError {
    Io(std::io::Error),
    Empty,
    BadLine { line: usize, reason: String },
    DuplicateKey { line: usize },
    DuplicateTenantKey { line: usize },
}

impl fmt::Display for KeyFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyFileError::Io(e) => write!(f, "cannot read api-key file: {e}"),
            KeyFileError::Empty => write!(
                f,
                "api-key file has no usable entries (need at least one `<tenant>:<key>` line); \
                 refusing to start an ingest surface no key can authenticate"
            ),
            KeyFileError::BadLine { line, reason } => {
                write!(f, "api-key file line {line}: {reason}")
            }
            KeyFileError::DuplicateKey { line } => write!(
                f,
                "api-key file line {line}: duplicate api key (a key must map to exactly one tenant)"
            ),
            KeyFileError::DuplicateTenantKey { line } => write!(
                f,
                "api-key file line {line}: duplicate `<tenant>:<key>` entry"
            ),
        }
    }
}

impl std::error::Error for KeyFileError {}

impl KeyTable {
    /// Parse the key-file contents. One entry per line; blank lines and lines whose
    /// first non-whitespace char is `#` are ignored. An entry is `<tenant>:<key>`:
    /// * `tenant` must satisfy the space grammar (`[a-z0-9-]`, start alphanumeric,
    ///   ≤64, not reserved) — the same names the read routes serve.
    /// * `key` is opaque but must be ≥ [`MIN_API_KEY_LEN`] chars and contain no
    ///   whitespace.
    ///
    /// A key may appear only once across the whole file (a key maps to one tenant).
    /// Fails on the first offending line with a line-numbered reason.
    pub fn parse(contents: &str) -> Result<Self, KeyFileError> {
        let mut entries: Vec<(String, String)> = Vec::new();
        for (i, raw) in contents.lines().enumerate() {
            let line = i + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let Some((tenant, key)) = trimmed.split_once(':') else {
                return Err(KeyFileError::BadLine {
                    line,
                    reason: "expected `<tenant>:<key>` (missing ':')".to_string(),
                });
            };
            let tenant = tenant.trim();
            // The key is intentionally NOT trimmed on the right beyond the split:
            // a key is taken verbatim after the first ':' with surrounding
            // whitespace removed, and any interior whitespace is rejected below so
            // there is exactly one canonical form to present as the bearer token.
            let key = key.trim();
            if !valid_space(tenant) {
                return Err(KeyFileError::BadLine {
                    line,
                    reason: format!(
                        "invalid tenant id {tenant:?}: must be lowercase [a-z0-9-], \
                         start alphanumeric, ≤64 chars, and not be a reserved name"
                    ),
                });
            }
            if key.is_empty() {
                return Err(KeyFileError::BadLine {
                    line,
                    reason: "empty api key".to_string(),
                });
            }
            if key.chars().any(char::is_whitespace) {
                return Err(KeyFileError::BadLine {
                    line,
                    reason: "api key must not contain whitespace".to_string(),
                });
            }
            if key.chars().count() < MIN_API_KEY_LEN {
                return Err(KeyFileError::BadLine {
                    line,
                    reason: format!(
                        "api key too short (< {MIN_API_KEY_LEN} chars); use a high-entropy key"
                    ),
                });
            }
            if entries.iter().any(|(_, k)| k == key) {
                return Err(KeyFileError::DuplicateKey { line });
            }
            if entries.iter().any(|(t, k)| t == tenant && k == key) {
                return Err(KeyFileError::DuplicateTenantKey { line });
            }
            entries.push((tenant.to_string(), key.to_string()));
        }
        if entries.is_empty() {
            return Err(KeyFileError::Empty);
        }
        Ok(KeyTable { entries })
    }

    /// Load + parse the key file at `path` (bounded read).
    pub fn load(path: &Path) -> Result<Self, KeyFileError> {
        let contents = std::fs::read_to_string(path).map_err(KeyFileError::Io)?;
        Self::parse(&contents)
    }

    /// Number of loaded keys (for the startup envelope; never the keys themselves).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Present so the `len`/`is_empty` pair is complete (clippy); the table is
    /// never empty after a successful `parse` (that returns [`KeyFileError::Empty`]).
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Resolve a presented bearer token to its tenant, or `None`. **Constant-time
    /// per key and non-short-circuiting across the table**: every stored key is
    /// compared with [`ct_eq`] and the loop never breaks early, so neither the
    /// matched key's bytes nor its position in the table leaks through timing. A
    /// match records the tenant; a token matching no key returns `None`.
    pub fn authenticate(&self, presented: &str) -> Option<Tenant> {
        let mut matched: Option<&str> = None;
        for (tenant, key) in &self.entries {
            // ct_eq for the content-timing guarantee; fold the result without an
            // early break so total time is independent of which row (if any) hit.
            if ct_eq(presented.as_bytes(), key.as_bytes()) {
                matched = Some(tenant);
            }
        }
        matched.map(|t| Tenant(t.to_string()))
    }
}

/// Extract a `Bearer <token>` from an `Authorization` header value. Strict: the
/// scheme must be exactly `Bearer` (case-insensitive per RFC 7235) followed by a
/// single space and a non-empty token. Returns `None` for a missing/mangled header
/// so the caller fails closed.
fn parse_bearer(value: &str) -> Option<&str> {
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    Some(token)
}

/// Ingest auth middleware. Rejects (401) any request lacking a valid
/// `Authorization: Bearer <key>` for a key in the table; on success inserts the
/// resolved [`Tenant`] into the request extensions and proceeds. Fail-closed: a
/// missing header, empty/whitespace token, wrong scheme, or unknown key all 401.
pub async fn ingest_auth(
    State(keys): State<Arc<KeyTable>>,
    mut req: Request,
    next: Next,
) -> Response {
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_bearer);

    let Some(token) = presented else {
        return unauthorized();
    };
    let Some(tenant) = keys.authenticate(token) else {
        return unauthorized();
    };
    req.extensions_mut().insert(tenant);
    next.run(req).await
}

fn unauthorized() -> Response {
    // No detail on *why* (missing vs. wrong key) — a uniform 401 gives an attacker
    // nothing. `WWW-Authenticate` advertises the scheme per RFC 7235.
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        "unauthorized",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    const K1: &str = "0123456789abcdef0123456789abcdef"; // 32 chars
    const K2: &str = "fedcba9876543210fedcba9876543210"; // 32 chars

    fn table() -> KeyTable {
        KeyTable::parse(&format!("# comment\nacme:{K1}\n\nglobex:{K2}\n")).unwrap()
    }

    #[test]
    fn parse_accepts_valid_entries_and_skips_comments_blanks() {
        let t = table();
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn authenticate_maps_key_to_tenant() {
        let t = table();
        assert_eq!(t.authenticate(K1), Some(Tenant("acme".into())));
        assert_eq!(t.authenticate(K2), Some(Tenant("globex".into())));
    }

    #[test]
    fn authenticate_rejects_unknown_empty_and_prefix() {
        let t = table();
        assert_eq!(t.authenticate("nope"), None);
        assert_eq!(t.authenticate(""), None);
        // A correct-prefix-but-wrong key must not authenticate.
        assert_eq!(t.authenticate(&K1[..31]), None);
        assert_eq!(t.authenticate(&format!("{K1}x")), None);
    }

    #[test]
    fn empty_file_is_error() {
        assert!(matches!(
            KeyTable::parse("# only comments\n\n"),
            Err(KeyFileError::Empty)
        ));
        assert!(matches!(KeyTable::parse(""), Err(KeyFileError::Empty)));
    }

    /// `KeyTable` deliberately does not implement `Debug` (it holds secret keys, so
    /// it must never be formattable), so these tests unwrap the error arm by hand
    /// rather than via `unwrap_err` (which needs `T: Debug`).
    fn parse_err(input: &str) -> KeyFileError {
        match KeyTable::parse(input) {
            Ok(_) => panic!("expected a parse error for {input:?}"),
            Err(e) => e,
        }
    }

    #[test]
    fn short_key_rejected() {
        let err = parse_err("acme:short");
        assert!(matches!(err, KeyFileError::BadLine { .. }));
        assert!(err.to_string().contains("too short"));
    }

    #[test]
    fn missing_colon_rejected() {
        let err = parse_err(&format!("acme{K1}"));
        assert!(err.to_string().contains("missing ':'"));
    }

    #[test]
    fn bad_tenant_rejected() {
        // Reserved / grammar-invalid tenant names are refused.
        assert!(KeyTable::parse(&format!("api:{K1}")).is_err()); // reserved
        assert!(KeyTable::parse(&format!("Bad_Name:{K1}")).is_err()); // grammar
    }

    #[test]
    fn duplicate_key_rejected() {
        let err = parse_err(&format!("acme:{K1}\nglobex:{K1}"));
        assert!(matches!(err, KeyFileError::DuplicateKey { line: 2 }));
    }

    #[test]
    fn whitespace_in_key_rejected() {
        assert!(KeyTable::parse("acme:has space in key aaaaaaaaaaaaaaaaaaaa").is_err());
    }

    #[test]
    fn parse_bearer_is_strict() {
        assert_eq!(parse_bearer("Bearer abc"), Some("abc"));
        assert_eq!(parse_bearer("bearer abc"), Some("abc")); // case-insensitive scheme
        assert_eq!(parse_bearer("Bearer   abc  "), Some("abc")); // token trimmed
        assert_eq!(parse_bearer("Bearer "), None); // empty token
        assert_eq!(parse_bearer("Basic abc"), None); // wrong scheme
        assert_eq!(parse_bearer("abc"), None); // no scheme
        assert_eq!(parse_bearer(""), None);
    }
}
