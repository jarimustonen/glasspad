//! Markdown + reusable-template render path (0.3.0 headline feature).
//!
//! The **single server-side renderer**: markdown body + a referenced template →
//! an artifact **body** string that the existing serve path hosts. Nothing here
//! touches the security boundary. The produced body is stored exactly like a
//! `create`d artifact's HTML and served on `/{space}/_c/{slug}`, where
//! `artifact_content` sets the CSP / sandbox / Trusted-Types / hardening headers
//! **on the response** (independent of body content) and `wrap::render_artifact`
//! wraps a **fragment** body into a themed document with `base.css` linked +
//! `bridge.js` injected. So a template governs only the artifact body — it can
//! never widen CSP, reach the trusted shell, or escape the null-origin sandbox
//! (design.md §7: the sandbox/CSP is the boundary, not sanitization). The
//! built-in templates are authored as fragments so they inherit the `--gp-*`
//! design system (incl. the hardened `.gp-prose` reading theme) for free.
//!
//! A template is plain HTML with **exactly one** `{{content}}` insertion point —
//! reusable, not a templating DSL. The default template is the `prose` theme,
//! which wraps the rendered markdown directly in `<article class="gp-prose">` so
//! rendered blocks are **direct children** of `.gp-prose` — the render contract
//! the `prose-theme` hardening (flushed first/last margins, wide-table scroll,
//! long-URL wrap, loose-list gaps, task-list checkboxes) is written against.

use std::fmt;

use pulldown_cmark::{Options, Parser, html};

/// The insertion point a template must carry, exactly once. Whitespace inside the
/// braces is tolerated (`{{ content }}` == `{{content}}`); see [`apply_template`].
pub const PLACEHOLDER: &str = "{{content}}";

/// A built-in template name (`--template prose|dashboard`). Public so the CLI can
/// report which built-in resolved and reject an unknown name with an `expected`
/// allowlist (AI-first §10).
pub const BUILTIN_NAMES: &[&str] = &["prose", "dashboard"];

/// The default template when `--template` is omitted: the reading theme.
pub const DEFAULT_TEMPLATE: &str = "prose";

/// Resolve a built-in template name to its HTML fragment, or `None` if the name is
/// not a built-in. Both fragments carry exactly one `{{content}}` and are
/// fragments (not full documents), so the content route wraps them with
/// `base.css` + `bridge.js` — the `--gp-*` system styles them with no extra
/// wiring.
///
/// * `prose` — `<article class="gp-prose">` so rendered blocks are direct children
///   of `.gp-prose` (the hardened reading theme's render contract).
/// * `dashboard` — the default dashboard look, content in a `.gp-card` surface.
pub fn builtin_template(name: &str) -> Option<&'static str> {
    match name {
        "prose" => Some("<article class=\"gp-prose\">\n{{content}}\n</article>\n"),
        "dashboard" => Some("<div class=\"gp-card\">\n{{content}}\n</div>\n"),
        _ => None,
    }
}

/// Render a markdown body to an HTML fragment. CommonMark + the GFM extensions
/// (tables, strikethrough, task lists, footnotes). Raw inline/block HTML in the
/// markdown **passes through** — acceptable because the output is served inside
/// the null-origin sandbox (it is untrusted script there regardless), and the
/// reason `.gp-prose` was hardened against arbitrary markdown-generated markup.
/// Infallible: `pulldown-cmark` never errors, it lossily parses any input.
pub fn render_markdown(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);
    // A markdown-authored artifact renders once, server-side; the extra parse cost
    // of a full walk is irrelevant, and a larger buffer avoids reallocs.
    let parser = Parser::new_ext(md, opts);
    let mut out = String::with_capacity(md.len() + md.len() / 2 + 64);
    html::push_html(&mut out, parser);
    out
}

/// Why a template could not be applied. Surfaced by the CLI as `invalid_template`.
#[derive(Debug, PartialEq, Eq)]
pub enum TemplateError {
    /// The template carries no `{{content}}` placeholder — the rendered markdown
    /// would have nowhere to go.
    MissingPlaceholder,
    /// The template carries more than one `{{content}}` placeholder — ambiguous
    /// (which slot receives the body?). A reusable template has exactly one.
    DuplicatePlaceholder(usize),
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::MissingPlaceholder => write!(
                f,
                "template has no {PLACEHOLDER} placeholder: a template must contain \
                 exactly one {PLACEHOLDER} where the rendered markdown is inserted"
            ),
            TemplateError::DuplicatePlaceholder(n) => write!(
                f,
                "template has {n} {PLACEHOLDER} placeholders: a template must contain \
                 exactly one (which slot would receive the body is ambiguous)"
            ),
        }
    }
}

impl std::error::Error for TemplateError {}

/// Splice `rendered` into `template` at its single `{{content}}` placeholder.
/// Whitespace inside the braces is tolerated, so `{{ content }}` matches too; any
/// other `{{…}}` token (e.g. a literal `{{ note }}` the author wrote) is left
/// verbatim. Exactly one `content` placeholder is required — zero or many is a
/// [`TemplateError`]. The rendered HTML is inserted **verbatim** (no escaping):
/// the result is the artifact body, already served inside the sandbox, so this is
/// not a trust boundary.
pub fn apply_template(template: &str, rendered: &str) -> Result<String, TemplateError> {
    let spans = content_placeholder_spans(template);
    match spans.len() {
        0 => Err(TemplateError::MissingPlaceholder),
        1 => {
            let (start, end) = spans[0];
            let mut out = String::with_capacity(template.len() + rendered.len());
            out.push_str(&template[..start]);
            out.push_str(rendered);
            out.push_str(&template[end..]);
            Ok(out)
        }
        n => Err(TemplateError::DuplicatePlaceholder(n)),
    }
}

/// Byte spans of every `{{…}}` whose inner text trims to exactly `content`. Scans
/// left to right without regex, so it is allocation-light and has no catastrophic
/// backtracking. An unterminated `{{` (no following `}}`) ends the scan.
fn content_placeholder_spans(template: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0;
    while let Some(rel_open) = template[i..].find("{{") {
        let open = i + rel_open;
        let after_open = open + 2;
        let Some(rel_close) = template[after_open..].find("}}") else {
            break; // no matching close — stop scanning
        };
        let close = after_open + rel_close;
        if template[after_open..close].trim() == "content" {
            spans.push((open, close + 2));
        }
        i = close + 2;
    }
    spans
}

/// The full render: markdown body + template string → artifact body. Renders the
/// markdown, then splices it into the template's single placeholder.
pub fn render_to_body(markdown: &str, template: &str) -> Result<String, TemplateError> {
    let rendered = render_markdown(markdown);
    apply_template(template, &rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_renders_common_blocks() {
        let html = render_markdown("# Title\n\nA **bold** para.\n");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn markdown_renders_gfm_table_strikethrough_and_tasklist() {
        let table = render_markdown("| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(table.contains("<table>") && table.contains("<td>1</td>"));
        let strike = render_markdown("~~gone~~");
        assert!(strike.contains("<del>gone</del>"));
        let tasks = render_markdown("- [x] done\n- [ ] todo\n");
        assert!(tasks.contains("type=\"checkbox\""));
        assert!(tasks.contains("checked"));
    }

    #[test]
    fn markdown_renders_code_fence() {
        let html = render_markdown("```\nlet x = 1;\n```\n");
        assert!(html.contains("<pre><code>"));
        assert!(html.contains("let x = 1;"));
    }

    #[test]
    fn builtin_prose_is_the_default_and_wraps_gp_prose() {
        assert_eq!(DEFAULT_TEMPLATE, "prose");
        let t = builtin_template("prose").unwrap();
        assert!(t.contains(r#"<article class="gp-prose">"#));
        assert!(t.contains(PLACEHOLDER));
        // The render contract: rendered blocks are DIRECT children of .gp-prose —
        // the placeholder sits immediately inside the article, nothing between.
        let body = render_to_body("# Hi\n\ntext", t).unwrap();
        let article_open = body.find(r#"<article class="gp-prose">"#).unwrap();
        let h1 = body.find("<h1>Hi</h1>").unwrap();
        // Only whitespace between the article tag and the first rendered block.
        let between = &body[article_open + r#"<article class="gp-prose">"#.len()..h1];
        assert!(between.trim().is_empty(), "not a direct child: {between:?}");
    }

    #[test]
    fn builtin_dashboard_uses_card_surface() {
        let t = builtin_template("dashboard").unwrap();
        assert!(t.contains(r#"class="gp-card""#));
        assert!(t.contains(PLACEHOLDER));
        assert!(builtin_template("nope").is_none());
    }

    #[test]
    fn apply_template_splices_single_placeholder() {
        let out = apply_template("<main>{{content}}</main>", "<p>x</p>").unwrap();
        assert_eq!(out, "<main><p>x</p></main>");
    }

    #[test]
    fn apply_template_tolerates_inner_whitespace() {
        let out = apply_template("A{{ content }}B", "X").unwrap();
        assert_eq!(out, "AXB");
    }

    #[test]
    fn apply_template_leaves_unrelated_braces_verbatim() {
        // A non-`content` `{{…}}` token is the author's literal text, untouched.
        let out = apply_template("{{ note }}<i>{{content}}</i>", "Z").unwrap();
        assert_eq!(out, "{{ note }}<i>Z</i>");
    }

    #[test]
    fn apply_template_rejects_missing_placeholder() {
        assert_eq!(
            apply_template("<main></main>", "x"),
            Err(TemplateError::MissingPlaceholder)
        );
        // An unterminated `{{` is not a placeholder.
        assert_eq!(
            apply_template("<main>{{content", "x"),
            Err(TemplateError::MissingPlaceholder)
        );
    }

    #[test]
    fn apply_template_rejects_duplicate_placeholder() {
        assert_eq!(
            apply_template("{{content}}{{ content }}", "x"),
            Err(TemplateError::DuplicatePlaceholder(2))
        );
    }

    #[test]
    fn render_to_body_end_to_end() {
        let out = render_to_body("## Sub\n", builtin_template("prose").unwrap()).unwrap();
        assert!(out.contains(r#"<article class="gp-prose">"#));
        assert!(out.contains("<h2>Sub</h2>"));
    }

    #[test]
    fn raw_html_in_markdown_passes_through() {
        // Passthrough is intentional (sandboxed output). We assert the behavior so
        // a future parser-option change that silently starts stripping HTML is caught.
        let html = render_markdown("<div class=\"x\">raw</div>\n");
        assert!(html.contains(r#"<div class="x">raw</div>"#));
    }
}
