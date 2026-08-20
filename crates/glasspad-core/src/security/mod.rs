//! Security primitives for the HTML-artifact host.
//!
//! Only [`token`] survives Wave 5: it generates the unguessable nonces the
//! artifact wrapper embeds. The old section-DSL sanitiser (`ammonia`-based HTML
//! scrubbing) and the JSON-script embedder existed only to safely inline
//! server-parsed data into the legacy dashboard renderer, which is gone — the
//! new model sandboxes every artifact in a null-origin iframe instead, so those
//! mechanisms were removed rather than demoted.

pub mod token;

/// Constant-time byte-slice equality for comparing secrets (API keys) without a
/// content-dependent early-out. For **equal-length** inputs the comparison time is
/// independent of *where* the first differing byte is, so a timing side channel
/// cannot walk the correct key out byte-by-byte. A length mismatch returns `false`
/// immediately: the length of a high-entropy (≥32-char random) API key is not a
/// meaningful secret, and revealing "wrong length" does not narrow the key search
/// space — this is the same trade the standard `constant_time_eq` primitive makes.
///
/// Callers that check a presented key against a *table* of keys must iterate the
/// whole table (no short-circuit) so total time does not reveal the matching row.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        // `black_box` on each XOR (not just the final result) stops the optimizer
        // from proving `diff` is already nonzero and short-circuiting the fold into
        // an early-out branch — which would reintroduce a content-dependent timing
        // path. Folding the whole slice unconditionally is the constant-time part.
        diff |= std::hint::black_box(x ^ y);
    }
    diff == 0
}

#[cfg(test)]
mod ct_tests {
    use super::ct_eq;

    #[test]
    fn equal_slices_match() {
        assert!(ct_eq(
            b"correct-horse-battery-staple-123",
            b"correct-horse-battery-staple-123"
        ));
        assert!(ct_eq(b"", b""));
    }

    #[test]
    fn different_content_same_length_fails() {
        assert!(!ct_eq(b"aaaaaaaaaaaaaaaa", b"aaaaaaaaaaaaaaab"));
        assert!(!ct_eq(b"baaaaaaaaaaaaaaa", b"aaaaaaaaaaaaaaaa"));
    }

    #[test]
    fn different_length_fails() {
        assert!(!ct_eq(b"short", b"shorter"));
        assert!(!ct_eq(b"", b"x"));
    }
}
