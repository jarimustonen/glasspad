/// Generate a safe `<script type="application/json">` tag for embedding data.
///
/// The JSON is NOT executable — it's parsed via `JSON.parse(element.textContent)`.
/// All `<` characters are escaped to `\u003c` to prevent any HTML context breakout,
/// including case-insensitive `</script>` variants.
pub fn safe_json_script_tag(id: &str, value: &serde_json::Value) -> String {
    let json = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    let escaped = escape_for_script_tag(&json);
    format!(
        "<script id=\"{}\" type=\"application/json\">{}</script>",
        html_escape_attr(id),
        escaped
    )
}

/// Escape all `<` characters in serialized JSON to prevent HTML context switching.
/// `\u003c` is valid JSON and will be correctly parsed by `JSON.parse`.
fn escape_for_script_tag(json: &str) -> String {
    json.replace('<', "\\u003c")
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
        assert!(tag.contains("\\u003c"));
    }

    #[test]
    fn escapes_mixed_case_script_tag() {
        let data = json!({"xss": "</sCrIpT><script>alert(1)</script>"});
        let tag = safe_json_script_tag("data", &data);
        assert!(!tag.contains("</sCrIpT>"));
        assert!(!tag.contains("<script>alert"));
    }

    #[test]
    fn escapes_html_comment() {
        let data = json!({"text": "<!-- comment -->"});
        let tag = safe_json_script_tag("data", &data);
        assert!(!tag.contains("<!--"));
    }

    #[test]
    fn escapes_all_angle_brackets() {
        let data = json!({"html": "<b>bold</b>"});
        let tag = safe_json_script_tag("data", &data);
        // All < should be escaped in JSON payload
        let payload_start = tag.find('>').unwrap() + 1;
        let payload_end = tag.rfind("</script>").unwrap();
        let payload = &tag[payload_start..payload_end];
        assert!(!payload.contains('<'));
    }

    #[test]
    fn escapes_id_special_chars() {
        let tag = safe_json_script_tag("a\"b<c", &json!(null));
        assert!(tag.contains("id=\"a&quot;b&lt;c\""));
    }

    #[test]
    fn roundtrip_with_json_parse() {
        let data = json!({"msg": "a</script>b", "html": "<b>test</b>"});
        let tag = safe_json_script_tag("test", &data);

        // Extract JSON payload
        let start = tag.find('>').unwrap() + 1;
        let end = tag.rfind("</script>").unwrap();
        let embedded_json = &tag[start..end];

        // \u003c is valid JSON — JSON.parse handles it natively
        let parsed: serde_json::Value = serde_json::from_str(embedded_json).unwrap();
        assert_eq!(parsed["msg"], "a</script>b");
        assert_eq!(parsed["html"], "<b>test</b>");
    }

    #[test]
    fn empty_object() {
        let tag = safe_json_script_tag("d", &json!({}));
        assert!(tag.contains("{}"));
    }
}
