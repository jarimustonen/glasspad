/// Generate a safe `<script type="application/json">` tag for embedding data.
///
/// The JSON is NOT executable — it's parsed via `JSON.parse(element.textContent)`.
/// The function escapes sequences that could break out of the script tag.
pub fn safe_json_script_tag(id: &str, value: &serde_json::Value) -> String {
    let json = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    let escaped = escape_for_script_tag(&json);
    format!(
        "<script id=\"{}\" type=\"application/json\">{}</script>",
        html_escape_attr(id),
        escaped
    )
}

/// Escape sequences that could break a `<script>` context:
/// - `</script>` → `<\/script>` (prevents premature tag close)
/// - `<!--` → `<\!--` (prevents HTML comment injection)
fn escape_for_script_tag(json: &str) -> String {
    json.replace("</script>", r"<\/script>")
        .replace("</SCRIPT>", r"<\/SCRIPT>")
        .replace("<!--", r"<\!--")
}

fn html_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn simple_data() {
        let data = json!({"count": 42});
        let tag = safe_json_script_tag("my-data", &data);
        assert!(tag.contains("type=\"application/json\""));
        assert!(tag.contains("id=\"my-data\""));
        assert!(tag.contains(r#"{"count":42}"#));
    }

    #[test]
    fn escapes_script_close_tag() {
        let data = json!({"html": "</script><script>alert(1)</script>"});
        let tag = safe_json_script_tag("data", &data);
        assert!(!tag.contains("</script><script>"));
        assert!(tag.contains(r"<\/script>"));
    }

    #[test]
    fn escapes_html_comment() {
        let data = json!({"text": "<!-- comment -->"});
        let tag = safe_json_script_tag("data", &data);
        assert!(!tag.contains("<!--"));
        assert!(tag.contains(r"<\!--"));
    }

    #[test]
    fn escapes_id_special_chars() {
        let tag = safe_json_script_tag("a\"b<c", &json!(null));
        assert!(tag.contains("id=\"a&quot;b&lt;c\""));
    }

    #[test]
    fn roundtrip_with_js_parse() {
        // Verify the escaped JSON is still valid JSON when unescaped
        let data = json!({"msg": "a</script>b"});
        let tag = safe_json_script_tag("test", &data);

        // Extract the JSON part between > and </script>
        let start = tag.find('>').unwrap() + 1;
        let end = tag.rfind("</script>").unwrap();
        let embedded_json = &tag[start..end];

        // The browser does textContent which gives the raw text
        // Our escaping uses \/ which is valid JSON
        let restored = embedded_json.replace(r"<\/script>", "</script>");
        let parsed: serde_json::Value = serde_json::from_str(&restored).unwrap();
        assert_eq!(parsed["msg"], "a</script>b");
    }

    #[test]
    fn empty_object() {
        let tag = safe_json_script_tag("d", &json!({}));
        assert!(tag.contains("{}"));
    }

    #[test]
    fn large_nested_data() {
        let data = json!({
            "events": [
                {"a": 1, "b": "hello"},
                {"a": 2, "b": "world"}
            ]
        });
        let tag = safe_json_script_tag("datasets", &data);
        assert!(tag.contains("events"));
        assert!(tag.contains("hello"));
    }
}
