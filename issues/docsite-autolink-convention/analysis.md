# Producer preprocessing seam and link-class finding

## Decision

Semantic rewriting belongs before glasspad scans or publishes a space. A producer may
transform source markdown into a generated markdown/HTML directory, then pass that
directory to `glasspad publish`, `serve`, or `build`. Glasspad does not own glossary
recognition, cross-reference resolution, citation lookup, or a producer's source syntax.

The producer-facing convention and a worked glossary/xref example are documented in the
README under **Markdown-native spaces → Producer preprocessing: glossary autolinks and
cross-references**.

## Confirmed renderer behavior

Author-supplied classes on raw HTML links survive unchanged. There is no link-class
allowlist and none is needed for the current rendering architecture:

1. `src/artifact_host/render.rs::render_markdown` uses `pulldown_cmark` with raw HTML
   enabled. Its documented and tested contract is that raw inline/block HTML passes
   through.
2. The prose path, `render_markdown_with_headings`, rewrites only Markdown heading start
   events to add generated IDs. Other events, including raw HTML, are passed to
   `pulldown_cmark::html::push_html` unchanged.
3. `apply_template` inserts rendered HTML verbatim. The built-in prose template only wraps
   it in `.gp-prose`; a custom space template does the same splice.
4. `src/artifact_host/wrap.rs::render_artifact` wraps a fragment with the base stylesheet
   and bridge. It does not sanitize artifact content. The null-origin sandbox and CSP are
   the security boundary, as stated in `src/artifact_host/AGENTS.md` and `render.rs`.

Consequences:

- `[label](page.md)` becomes a normal `<a href="page.md">` without a class because
  CommonMark has no link-class syntax.
- `<a class="xref glossary-term" href="glossary.html#term">label</a>` in markdown remains
  an anchor with both classes in the rendered prose page.
- Arbitrary author classes survive, not only `xref` or `glossary-term`. This is artifact
  content, not trusted shell markup and not an increase in authority.
- A custom fragment template can include inline CSS such as `.gp-prose a.xref { … }`;
  artifact CSP already permits inline styles.

No sanitizer, header, CSP, bridge, or host code was changed for this issue. Adding a class
allowlist would incorrectly imply sanitization and would narrow an existing raw-HTML
passthrough contract without providing a security benefit.
