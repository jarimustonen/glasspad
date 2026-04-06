use rand::RngCore;

/// Generate a cryptographically random 32-character hex token.
pub fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Constant-time token comparison to prevent timing attacks.
pub fn verify_token(provided: &str, expected: &str) -> bool {
    if provided.len() != expected.len() {
        return false;
    }
    let mut result: u8 = 0;
    for (a, b) in provided.bytes().zip(expected.bytes()) {
        result |= a ^ b;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_length() {
        let token = generate_token();
        assert_eq!(token.len(), 32);
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
    fn verify_different_lengths() {
        assert!(!verify_token("short", "longer_token"));
    }

    #[test]
    fn verify_empty_tokens() {
        assert!(verify_token("", ""));
    }
}
