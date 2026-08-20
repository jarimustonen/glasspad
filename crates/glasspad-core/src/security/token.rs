use rand::RngCore;

const TOKEN_LENGTH: usize = 32;

/// Generate a cryptographically random 32-character hex token.
pub fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Constant-time token comparison.
/// Rejects empty tokens and wrong-length tokens.
pub fn verify_token(provided: &str, expected: &str) -> bool {
    // Tokens must be exactly TOKEN_LENGTH characters
    if provided.len() != TOKEN_LENGTH || expected.len() != TOKEN_LENGTH {
        return false;
    }
    // Constant-time XOR comparison
    let mut result: u8 = 0;
    for (a, b) in provided.bytes().zip(expected.bytes()) {
        result |= a ^ b;
    }
    // Use black_box to prevent compiler from optimizing the comparison
    std::hint::black_box(result) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_length() {
        assert_eq!(generate_token().len(), TOKEN_LENGTH);
    }

    #[test]
    fn token_is_hex() {
        let token = generate_token();
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn tokens_are_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
    }

    #[test]
    fn verify_matching_tokens() {
        let token = generate_token();
        assert!(verify_token(&token, &token));
    }

    #[test]
    fn verify_different_tokens() {
        let a = generate_token();
        let b = generate_token();
        assert!(!verify_token(&a, &b));
    }

    #[test]
    fn reject_short_tokens() {
        assert!(!verify_token("short", "short"));
    }

    #[test]
    fn reject_empty_tokens() {
        assert!(!verify_token("", ""));
    }

    #[test]
    fn reject_wrong_length_provided() {
        let token = generate_token();
        assert!(!verify_token("abc", &token));
    }

    #[test]
    fn reject_wrong_length_expected() {
        let token = generate_token();
        assert!(!verify_token(&token, "abc"));
    }
}
