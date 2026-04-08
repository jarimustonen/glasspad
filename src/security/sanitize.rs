use std::collections::HashSet;

/// Sanitize HTML with a strict allowlist.
/// Only structural/text formatting tags are allowed. No scripts, forms, iframes, etc.
pub fn sanitize_html(input: &str) -> String {
    let mut allowed_tags = HashSet::new();
    for tag in &[
        "p", "br", "strong", "b", "em", "i", "u", "a",
        "ul", "ol", "li",
        "h1", "h2", "h3", "h4", "h5", "h6",
        "blockquote", "pre", "code",
        "table", "thead", "tbody", "tr", "th", "td",
        "div", "span", "hr",
        "img",
    ] {
        allowed_tags.insert(*tag);
    }

    let mut builder = ammonia::Builder::new();
    builder.tags(allowed_tags);

    // Allow href on <a>, src/alt/width/height on <img>
    let mut allowed_attrs = std::collections::HashMap::new();
    let mut a_attrs = HashSet::new();
    a_attrs.insert("href");
    allowed_attrs.insert("a", a_attrs);
    let mut img_attrs = HashSet::new();
    img_attrs.insert("src");
    img_attrs.insert("alt");
    img_attrs.insert("width");
    img_attrs.insert("height");
    allowed_attrs.insert("img", img_attrs);
    builder.tag_attributes(allowed_attrs);

    // Allowed URL schemes for links and images
    let mut url_schemes = HashSet::new();
    url_schemes.insert("http");
    url_schemes.insert("https");
    url_schemes.insert("mailto");
    url_schemes.insert("cid");    // email inline images
    url_schemes.insert("data");   // data: URIs for embedded images
    builder.url_schemes(url_schemes);

    // Force target=_blank on links for safety
    builder.link_rel(Some("noopener noreferrer"));

    builder.clean(input).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_basic_formatting() {
        let input = "<p>Hello <strong>world</strong></p>";
        let result = sanitize_html(input);
        assert_eq!(result, "<p>Hello <strong>world</strong></p>");
    }

    #[test]
    fn allows_links() {
        let input = r#"<a href="https://example.com">Link</a>"#;
        let result = sanitize_html(input);
        assert!(result.contains("https://example.com"));
        assert!(result.contains("noopener noreferrer"));
    }

    #[test]
    fn strips_script_tags() {
        let input = "<p>Hello</p><script>alert(1)</script><p>World</p>";
        let result = sanitize_html(input);
        assert!(!result.contains("script"));
        assert!(!result.contains("alert"));
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
    }

    #[test]
    fn strips_event_handlers() {
        let input = r#"<p onclick="alert(1)">Click me</p>"#;
        let result = sanitize_html(input);
        assert!(!result.contains("onclick"));
        assert!(result.contains("Click me"));
    }

    #[test]
    fn strips_iframe() {
        let input = r#"<iframe src="https://evil.com"></iframe>"#;
        let result = sanitize_html(input);
        assert!(!result.contains("iframe"));
        assert!(!result.contains("evil.com"));
    }

    #[test]
    fn strips_style_attribute() {
        let input = r#"<p style="color:red">Text</p>"#;
        let result = sanitize_html(input);
        assert!(!result.contains("style"));
        assert!(result.contains("Text"));
    }

    #[test]
    fn allows_img_with_safe_attrs() {
        let input = r#"<img src="https://example.com/photo.jpg" alt="Photo" width="100">"#;
        let result = sanitize_html(input);
        assert!(result.contains("img"));
        assert!(result.contains("https://example.com/photo.jpg"));
        assert!(result.contains("alt"));
    }

    #[test]
    fn strips_img_onerror() {
        let input = r#"<img src="x" onerror="alert(1)">"#;
        let result = sanitize_html(input);
        assert!(!result.contains("onerror"));
        assert!(!result.contains("alert"));
        // img tag itself is kept, but dangerous attrs stripped
        assert!(result.contains("img"));
    }

    #[test]
    fn strips_form() {
        let input = r#"<form action="https://evil.com"><input type="submit"></form>"#;
        let result = sanitize_html(input);
        assert!(!result.contains("form"));
        assert!(!result.contains("evil.com"));
    }

    #[test]
    fn allows_lists() {
        let input = "<ul><li>One</li><li>Two</li></ul>";
        let result = sanitize_html(input);
        assert_eq!(result, input);
    }

    #[test]
    fn allows_headings() {
        let input = "<h1>Title</h1><h3>Subtitle</h3>";
        let result = sanitize_html(input);
        assert_eq!(result, input);
    }

    #[test]
    fn allows_blockquote_and_code() {
        let input = "<blockquote>Quote</blockquote><pre><code>code</code></pre>";
        let result = sanitize_html(input);
        assert!(result.contains("<blockquote>"));
        assert!(result.contains("<code>"));
    }

    #[test]
    fn strips_javascript_url() {
        let input = r#"<a href="javascript:alert(1)">XSS</a>"#;
        let result = sanitize_html(input);
        assert!(!result.contains("javascript"));
    }

    #[test]
    fn preserves_text_content() {
        let input = "Plain text without any tags";
        let result = sanitize_html(input);
        assert_eq!(result, input);
    }

    #[test]
    fn handles_empty_input() {
        assert_eq!(sanitize_html(""), "");
    }

    #[test]
    fn strips_nested_xss() {
        let input = r#"<div><p>Safe</p><script>alert("xss")</script><p>Also safe</p></div>"#;
        let result = sanitize_html(input);
        assert!(!result.contains("script"));
        assert!(result.contains("Safe"));
        assert!(result.contains("Also safe"));
    }
}
