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
//!
//! **Per-page TOC rail (prose-page-toc).** The built-in `prose` path additionally
//! stamps a server-generated anchor `id` on every heading (slugify + deterministic
//! collision disambiguation — never an attacker-controlled raw id) and, when the page
//! carries ≥2 H2/H3 headings, emits an "on this page" `<nav class="gp-toc">` rail as a
//! **sibling** of `.gp-prose` inside a `.gp-doc` grid. This is approach (a): the rail
//! lives inside the artifact's OWN fragment, so its `#anchor` links resolve natively
//! inside the null-origin sandbox — no shell involvement, no new postMessage surface,
//! CSP unchanged. Heading text is untrusted and reaches the rail only HTML-escaped.
//! Every other template (`dashboard`, custom) is the unchanged plain splice.

use std::collections::HashMap;
use std::fmt;

use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};

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
    // A markdown-authored artifact renders once, server-side; the extra parse cost
    // of a full walk is irrelevant, and a larger buffer avoids reallocs.
    let parser = Parser::new_ext(md, gfm_options());
    let mut out = String::with_capacity(md.len() + md.len() / 2 + 64);
    html::push_html(&mut out, parser);
    out
}

/// The GFM parse options shared by every markdown render path (the plain
/// [`render_markdown`] and the heading-anchoring [`render_markdown_with_headings`])
/// so the two never drift.
fn gfm_options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts
}

/// The lowest number of H2/H3 headings a page needs before the "on this page" rail
/// is worth rendering. One entry is not a table of contents, so a single-heading (or
/// heading-less) page degrades to the plain prose fragment — no empty rail, byte-for-
/// byte the pre-TOC layout apart from the (harmless, deep-link-enabling) heading ids.
const MIN_TOC_ENTRIES: usize = 2;

/// One entry in the per-page "on this page" table of contents: a server-derived H2 or
/// H3. `id` is the slugified, collision-disambiguated anchor emitted on the heading in
/// the body; `text` is the heading's plain text (untrusted — escaped at render time).
struct TocEntry {
    /// Heading depth, `2` or `3`, used to indent the rail entry.
    level: u8,
    /// The server-generated anchor id (slug), matching the `id=` on the heading.
    id: String,
    /// The heading's plain text — artifact-derived, HTML-escaped before it hits the DOM.
    text: String,
}

/// Render a markdown body to an HTML fragment **and** stamp a stable `id` on every
/// heading, collecting the H2/H3 headings as [`TocEntry`]s for the rail.
///
/// The ids are generated **server-side** from the heading text (slugify +
/// deterministic collision disambiguation) — never an attacker-controlled raw id —
/// and are written through `pulldown-cmark`'s own HTML-escaping heading-id path
/// (`Tag::Heading.id`), so the same null-origin-sandbox trust story as
/// [`render_markdown`] holds. In-page `#anchor` links then resolve natively inside the
/// artifact iframe with no shell involvement.
fn render_markdown_with_headings(md: &str) -> (String, Vec<TocEntry>) {
    // Materialize the event stream so a heading's id can be rewritten *before* it is
    // serialized: the slug is derived from the heading's inner text, which only arrives
    // in the events *after* the `Start(Heading)`. Buffering the whole stream also lets
    // the final `push_html` run as a SINGLE call, preserving its internal list/newline
    // state exactly as `render_markdown` produces it.
    let mut events: Vec<Event> = Parser::new_ext(md, gfm_options()).collect();
    let mut toc: Vec<TocEntry> = Vec::new();
    let mut used_ids: HashMap<String, usize> = HashMap::new();

    let mut i = 0;
    while i < events.len() {
        let Event::Start(Tag::Heading { level, .. }) = &events[i] else {
            i += 1;
            continue;
        };
        let level = *level;
        // Accumulate the heading's plain text (Text + inline Code) up to its close.
        let mut text = String::new();
        let mut j = i + 1;
        while j < events.len() {
            match &events[j] {
                Event::End(TagEnd::Heading(_)) => break,
                Event::Text(t) | Event::Code(t) => text.push_str(t),
                _ => {}
            }
            j += 1;
        }
        let text = text.trim().to_string();
        let id = unique_slug(&text, &mut used_ids);
        // Stamp the id on the heading start event; `push_html` HTML-escapes it.
        if let Event::Start(Tag::Heading { id: hid, .. }) = &mut events[i] {
            *hid = Some(CowStr::from(id.clone()));
        }
        if matches!(level, HeadingLevel::H2 | HeadingLevel::H3) {
            let depth = if level == HeadingLevel::H2 { 2 } else { 3 };
            toc.push(TocEntry {
                level: depth,
                id,
                text,
            });
        }
        i = j + 1;
    }

    let mut out = String::with_capacity(md.len() + md.len() / 2 + 64);
    html::push_html(&mut out, events.into_iter());
    (out, toc)
}

/// Slugify `text` into an anchor id and disambiguate it against ids already used on
/// this page, deterministically: lowercase, every run of non-alphanumeric characters
/// collapses to a single `-`, and leading/trailing `-` are trimmed. Unicode letters
/// are kept (Finnish `ä`/`ö` stay readable in the anchor). An empty result (a heading
/// of only punctuation/emoji) falls back to `section`. A repeat of an already-used
/// slug gets `-1`, `-2`, … appended — so ids are unique and stable for a given
/// document order, and never attacker-controlled raw markup.
fn unique_slug(text: &str, used: &mut HashMap<String, usize>) -> String {
    let mut base = String::with_capacity(text.len());
    let mut prev_dash = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                base.push(lower);
            }
            prev_dash = false;
        } else if !prev_dash {
            base.push('-');
            prev_dash = true;
        }
    }
    let base = base.trim_matches('-');
    let base = if base.is_empty() { "section" } else { base };

    match used.get_mut(base) {
        Some(n) => {
            *n += 1;
            format!("{base}-{n}")
        }
        None => {
            used.insert(base.to_string(), 0);
            base.to_string()
        }
    }
}

/// Build the "on this page" rail from the collected H2/H3 headings. The heading text
/// is **untrusted / artifact-derived**, so it reaches the DOM only HTML-escaped; the
/// href is `#<id>` where `<id>` is a server-generated slug (alphanumeric + `-`), also
/// escaped for defense in depth. Rendered as a native `<details>` (collapsible with no
/// JS — no new script, no shell surface); CSS hides it below the width breakpoint.
fn render_toc(entries: &[TocEntry]) -> String {
    let mut out = String::with_capacity(64 + entries.len() * 48);
    out.push_str("<nav class=\"gp-toc\" aria-label=\"On this page\">\n");
    out.push_str("<details class=\"gp-toc-panel\" open>\n");
    out.push_str("<summary class=\"gp-toc-title\">On this page</summary>\n");
    out.push_str("<ul class=\"gp-toc-list\">\n");
    for e in entries {
        out.push_str("<li class=\"gp-toc-l");
        out.push(if e.level == 2 { '2' } else { '3' });
        out.push_str("\"><a href=\"#");
        escape_into(&mut out, &e.id);
        out.push_str("\">");
        escape_into(&mut out, &e.text);
        out.push_str("</a></li>\n");
    }
    out.push_str("</ul>\n</details>\n</nav>\n");
    out
}

/// Render a markdown body as the built-in `prose` fragment, with the per-page TOC
/// rail. When the page has fewer than [`MIN_TOC_ENTRIES`] H2/H3 headings the rail is
/// omitted and the output is the plain `<article class="gp-prose">…</article>`
/// fragment (the pre-TOC layout) — graceful fallback, no empty rail. With enough
/// headings, the article and the rail sit side by side inside a `.gp-doc` grid (the
/// rail is a sibling of `.gp-prose`, so the "rendered blocks are direct children of
/// `.gp-prose`" render contract is preserved). Everything is one artifact fragment —
/// the anchors work natively inside the null-origin sandbox, no shell involvement.
fn render_prose_body(markdown: &str) -> String {
    let (rendered, toc) = render_markdown_with_headings(markdown);
    if toc.len() < MIN_TOC_ENTRIES {
        return format!("<article class=\"gp-prose\">\n{rendered}\n</article>\n");
    }
    let rail = render_toc(&toc);
    format!(
        "<div class=\"gp-doc\">\n<article class=\"gp-prose\">\n{rendered}\n</article>\n{rail}</div>\n"
    )
}

/// HTML-escape `s` (text + attribute contexts) into `out`. Mirrors the escaping used
/// elsewhere in the host for artifact-derived strings that reach a markup sink.
fn escape_into(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
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

/// Split `template` into `(prefix, suffix)` around its single `{{content}}`
/// placeholder — the validated form the splice needs. Whitespace inside the braces
/// is tolerated, so `{{ content }}` matches too; any other `{{…}}` token (a literal
/// `{{ note }}` the author wrote) is left in the prefix/suffix verbatim. Exactly one
/// `content` placeholder is required — zero or many is a [`TemplateError`].
///
/// The scan is a single left-to-right pass without regex, so no catastrophic
/// backtracking and no per-placeholder allocation (the duplicate count is tracked
/// as a counter, not an all-spans vector). On a `{{…}}` whose inner text is NOT
/// `content`, the cursor resumes just past the opening `{{` — so a stray/unrelated
/// `{{` before the real placeholder does **not** swallow it (e.g.
/// `A {{ x {{content}} y` still finds the real one). An unterminated `{{` (no
/// following `}}`) ends the scan.
fn split_at_placeholder(template: &str) -> Result<(&str, &str), TemplateError> {
    let mut first: Option<(usize, usize)> = None;
    let mut count = 0usize;
    let mut i = 0;
    while let Some(rel_open) = template[i..].find("{{") {
        let open = i + rel_open;
        let after_open = open + 2;
        let Some(rel_close) = template[after_open..].find("}}") else {
            break; // no matching close — stop scanning
        };
        let close = after_open + rel_close;
        if template[after_open..close].trim() == "content" {
            count += 1;
            if first.is_none() {
                first = Some((open, close + 2));
            }
            i = close + 2; // consume this placeholder, resume after it
        } else {
            // Not our placeholder: resume just past `{{` so a later/nested `{{…}}`
            // (the real one) is still found rather than skipped past its `}}`.
            i = after_open;
        }
    }
    match (first, count) {
        (None, _) => Err(TemplateError::MissingPlaceholder),
        (Some((start, end)), 1) => Ok((&template[..start], &template[end..])),
        (Some(_), n) => Err(TemplateError::DuplicatePlaceholder(n)),
    }
}

/// Splice `rendered` into `template` at its single `{{content}}` placeholder. The
/// rendered HTML is inserted **verbatim** (no escaping): the result is the artifact
/// body, already served inside the sandbox, so this is not a trust boundary.
pub fn apply_template(template: &str, rendered: &str) -> Result<String, TemplateError> {
    let (prefix, suffix) = split_at_placeholder(template)?;
    let mut out = String::with_capacity(prefix.len() + rendered.len() + suffix.len());
    out.push_str(prefix);
    out.push_str(rendered);
    out.push_str(suffix);
    Ok(out)
}

/// The full render: markdown body + template string → artifact body. Validates the
/// template's placeholder **first** (so a deterministically-invalid template is
/// rejected before the potentially-expensive markdown render), then renders the
/// markdown and splices it in.
pub fn render_to_body(markdown: &str, template: &str) -> Result<String, TemplateError> {
    // The built-in `prose` reading theme gets the heading-anchored, TOC-aware layout
    // (approach (a): the rail lives inside the artifact's own prose fragment). Every
    // other template — `dashboard`, or a client-supplied custom template — is the
    // unchanged plain splice, so its output is byte-for-byte what it was pre-TOC.
    if template == builtin_template(DEFAULT_TEMPLATE).expect("prose is a built-in") {
        return Ok(render_prose_body(markdown));
    }
    // Validate the single placeholder up front (cheap) so a bad template short-
    // circuits before rendering a possibly-large markdown body; then splice.
    split_at_placeholder(template)?;
    apply_template(template, &render_markdown(markdown))
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
        // The prose path now stamps a server-generated anchor id on every heading.
        let h1 = body.find(r#"<h1 id="hi">Hi</h1>"#).unwrap();
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
    fn apply_template_finds_placeholder_after_a_stray_open_brace() {
        // A stray/unrelated `{{ … }}` before the real placeholder must NOT swallow
        // it: the scanner resumes past `{{`, so the real `{{content}}` is found.
        let out = apply_template("A {{ note }} B {{content}} C", "X").unwrap();
        assert_eq!(out, "A {{ note }} B X C");
        // …even when the stray open shares the real placeholder's close scan window.
        let out = apply_template("{{ wrap {{content}} }}", "X").unwrap();
        assert_eq!(out, "{{ wrap X }}");
    }

    #[test]
    fn render_to_body_end_to_end() {
        let out = render_to_body("## Sub\n", builtin_template("prose").unwrap()).unwrap();
        assert!(out.contains(r#"<article class="gp-prose">"#));
        // A single heading gets an anchor id but no rail (below MIN_TOC_ENTRIES).
        assert!(out.contains(r#"<h2 id="sub">Sub</h2>"#));
        assert!(!out.contains("gp-toc"));
        assert!(!out.contains("gp-doc"));
    }

    #[test]
    fn prose_toc_rail_lists_h2_h3_with_anchors() {
        let md = "## Alpha\n\ntext\n\n### Beta\n\nmore\n\n## Gamma\n";
        let out = render_to_body(md, builtin_template("prose").unwrap()).unwrap();
        // Layout: the rail is a sibling of the prose article inside .gp-doc.
        assert!(out.contains(r#"<div class="gp-doc">"#));
        assert!(out.contains(r#"<article class="gp-prose">"#));
        assert!(out.contains(r#"<nav class="gp-toc""#));
        // Headings carry server-generated anchor ids…
        assert!(out.contains(r#"<h2 id="alpha">Alpha</h2>"#));
        assert!(out.contains(r#"<h3 id="beta">Beta</h3>"#));
        assert!(out.contains(r#"<h2 id="gamma">Gamma</h2>"#));
        // …and the rail links to them, indented by level.
        assert!(out.contains(r##"<li class="gp-toc-l2"><a href="#alpha">Alpha</a></li>"##));
        assert!(out.contains(r##"<li class="gp-toc-l3"><a href="#beta">Beta</a></li>"##));
        assert!(out.contains(r##"<li class="gp-toc-l2"><a href="#gamma">Gamma</a></li>"##));
    }

    #[test]
    fn prose_toc_needs_two_entries_else_degrades() {
        // Zero H2/H3 → no rail, plain prose fragment (the pre-TOC layout).
        let none =
            render_to_body("# Title\n\njust body\n", builtin_template("prose").unwrap()).unwrap();
        assert!(!none.contains("gp-doc") && !none.contains("gp-toc"));
        assert!(none.starts_with(r#"<article class="gp-prose">"#));
        // One H2 → still no rail (one entry is not a table of contents).
        let one = render_to_body("## Only\n\nbody\n", builtin_template("prose").unwrap()).unwrap();
        assert!(!one.contains("gp-doc") && !one.contains("gp-toc"));
    }

    #[test]
    fn prose_toc_only_lists_h2_h3_not_h1_h4() {
        let md = "# H1\n\n## Two\n\n#### Four\n\n## Three\n";
        let out = render_to_body(md, builtin_template("prose").unwrap()).unwrap();
        // All headings get ids (deep-link targets)…
        assert!(out.contains(r#"<h1 id="h1">H1</h1>"#));
        assert!(out.contains(r#"<h4 id="four">Four</h4>"#));
        // …but only H2/H3 appear in the rail.
        assert!(out.contains(r##"href="#two""##));
        assert!(out.contains(r##"href="#three""##));
        assert!(!out.contains(r##"href="#h1""##));
        assert!(!out.contains(r##"href="#four""##));
    }

    #[test]
    fn prose_toc_disambiguates_duplicate_headings() {
        let md = "## Setup\n\na\n\n## Setup\n\nb\n\n## Setup\n";
        let out = render_to_body(md, builtin_template("prose").unwrap()).unwrap();
        assert!(out.contains(r#"<h2 id="setup">Setup</h2>"#));
        assert!(out.contains(r#"<h2 id="setup-1">Setup</h2>"#));
        assert!(out.contains(r#"<h2 id="setup-2">Setup</h2>"#));
        assert!(out.contains(r##"href="#setup""##));
        assert!(out.contains(r##"href="#setup-1""##));
        assert!(out.contains(r##"href="#setup-2""##));
    }

    #[test]
    fn prose_toc_escapes_hostile_heading_text() {
        // Heading text is artifact-derived / untrusted: any `< > & " '` in it must
        // reach the rail DOM only HTML-escaped, never as a raw markup sink. (Actual
        // inline HTML in a heading is a non-Text event and never reaches the rail at
        // all — this covers the plain-text special-char path.)
        let md = "## Cost > 5 & \"risk\"\n\na\n\n## Q&A 'notes'\n";
        let out = render_to_body(md, builtin_template("prose").unwrap()).unwrap();
        // The rail text is escaped — no raw angle bracket / quote survives.
        assert!(out.contains("Cost &gt; 5 &amp; &quot;risk&quot;"));
        assert!(out.contains("Q&amp;A &#39;notes&#39;"));
        // The slug is alphanumeric + `-` only — nothing hostile leaks into the href.
        assert!(out.contains(r##"href="#cost-5-risk""##));
        assert!(out.contains(r##"href="#q-a-notes""##));
    }

    #[test]
    fn slugify_keeps_unicode_letters() {
        let mut used = HashMap::new();
        assert_eq!(
            unique_slug("Ääkköset ja Öljy", &mut used),
            "ääkköset-ja-öljy"
        );
        // Punctuation-only heading falls back to a stable slug.
        assert_eq!(unique_slug("!!! ???", &mut used), "section");
    }

    #[test]
    fn raw_html_in_markdown_passes_through() {
        // Passthrough is intentional (sandboxed output). We assert the behavior so
        // a future parser-option change that silently starts stripping HTML is caught.
        let html = render_markdown("<div class=\"x\">raw</div>\n");
        assert!(html.contains(r#"<div class="x">raw</div>"#));
    }
}
