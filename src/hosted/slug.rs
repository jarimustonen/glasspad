//! Capability-slug generation for the hosted share server.
//!
//! A published page's slug **is** its capability: reads are public-by-design
//! ("hold the link"; no read auth), so the slug must be unguessable and
//! enumeration-resistant. We draw 128 bits from the OS CSPRNG and encode them as
//! lowercase RFC-4648 base32 (`a-z2-7`), yielding a 26-char slug that satisfies the
//! existing space/slug grammar (`artifact_host::valid_name`: starts alphanumeric,
//! `[a-z0-9-]`, ≤64). 128 bits makes guessing/enumeration infeasible. Slugs are
//! never derived from content (no oracle) and never sequential.

use rand::RngCore;

/// The number of random bytes behind a slug (128 bits of entropy).
const SLUG_BYTES: usize = 16;

/// Lowercase RFC-4648 base32 alphabet (no padding). Every character is in the
/// `[a-z2-7]` subset of the slug grammar, and the first character is always a
/// letter or digit, so the encoding is a valid space name by construction.
const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Generate one fresh capability slug (26 lowercase base32 chars over 128 bits).
pub fn generate() -> String {
    let mut bytes = [0u8; SLUG_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    base32_encode(&bytes)
}

/// Encode bytes as lowercase base32 (no padding). 16 bytes → 26 chars (the last
/// char carries the final 2 bits). Pure and deterministic; unit-tested against
/// known vectors so a regression in the bit-packing is caught.
fn base32_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(5) * 8);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &b in input {
        buffer = (buffer << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1f) as usize;
            out.push(BASE32[idx] as char);
        }
    }
    if bits > 0 {
        // Left-align the remaining bits into a final 5-bit group (low bits zero).
        let idx = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(BASE32[idx] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact_host::valid_name;
    use std::collections::HashSet;

    #[test]
    fn slug_is_valid_name_and_expected_length() {
        for _ in 0..1000 {
            let s = generate();
            assert_eq!(s.len(), 26, "16 bytes → 26 base32 chars: {s}");
            assert!(valid_name(&s), "slug must satisfy the space grammar: {s}");
            assert!(
                s.bytes()
                    .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b)),
                "slug charset must be [a-z2-7]: {s}"
            );
        }
    }

    #[test]
    fn slugs_are_unique_across_many_draws() {
        // 128-bit slugs collide with negligible probability; a duplicate in 100k
        // draws would signal a broken RNG or encoder.
        let mut seen = HashSet::new();
        for _ in 0..100_000 {
            assert!(seen.insert(generate()), "unexpected slug collision");
        }
    }

    #[test]
    fn base32_known_vectors() {
        // RFC 4648 test vectors (lowercased, padding stripped).
        assert_eq!(base32_encode(b""), "");
        assert_eq!(base32_encode(b"f"), "my");
        assert_eq!(base32_encode(b"fo"), "mzxq");
        assert_eq!(base32_encode(b"foo"), "mzxw6");
        assert_eq!(base32_encode(b"foob"), "mzxw6yq");
        assert_eq!(base32_encode(b"fooba"), "mzxw6ytb");
        assert_eq!(base32_encode(b"foobar"), "mzxw6ytboi");
    }
}
