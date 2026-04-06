/// Content-Security-Policy header value for rendered pad pages.
///
/// Allows:
/// - Self-hosted resources
/// - Vega-Lite from jsdelivr CDN
/// - Inline styles (needed for Vega-Lite rendered charts)
/// - Data URIs for images (Vega-Lite SVG export)
/// - Connect to self (for future fetch-based data loading)
pub const CSP_HEADER_VALUE: &str = "\
default-src 'self'; \
script-src 'self' https://cdn.jsdelivr.net; \
style-src 'self' 'unsafe-inline'; \
connect-src 'self'; \
img-src 'self' data:; \
font-src 'self'; \
object-src 'none'; \
frame-src 'none'";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csp_contains_required_directives() {
        assert!(CSP_HEADER_VALUE.contains("default-src"));
        assert!(CSP_HEADER_VALUE.contains("script-src"));
        assert!(CSP_HEADER_VALUE.contains("style-src"));
        assert!(CSP_HEADER_VALUE.contains("connect-src"));
    }

    #[test]
    fn csp_allows_jsdelivr() {
        assert!(CSP_HEADER_VALUE.contains("https://cdn.jsdelivr.net"));
    }

    #[test]
    fn csp_blocks_frames() {
        assert!(CSP_HEADER_VALUE.contains("frame-src 'none'"));
    }

    #[test]
    fn csp_blocks_objects() {
        assert!(CSP_HEADER_VALUE.contains("object-src 'none'"));
    }
}
