# Technical contract

Treat these as hard constraints. Glasspad's engineers will handle integration after design handoff.

## Fragment shape

- Return a body **fragment**, not a complete document. Do not include `<!DOCTYPE>`, `<html>`, `<head>`, or `<body>`.
- Include exactly one literal `{{content}}` marker. It is the only substitution point; there is no template language.
- Keep each template in one file. Inline `<style>` is expected. Use inline JavaScript only when it materially improves the design and the no-script result remains useful.
- If the fragment uses `.gp-prose`, put `{{content}}` directly inside that element. The rendered Markdown blocks must be direct children of `.gp-prose`; an extra wrapper breaks reading-rhythm and table-of-contents behavior.

Correct:

```html
<article class="gp-prose">
  {{content}}
</article>
```

Incorrect:

```html
<article class="gp-prose"><div>{{content}}</div></article>
```

## Styling and assets

- Use the supplied `--gp-*` tokens for color, spacing-related system values, type, borders, shadows, and chart chrome. Do not invent a separate palette or hard-code screen colors.
- Use system font stacks only. Never load a web font.
- Do not depend on a CDN, remote image, external stylesheet, API, or network request. The artifact runs with `connect-src 'none'` under a strict Content Security Policy.
- It runs in a sandboxed, null-origin iframe. It cannot reach or style the trusted Glasspad shell.
- Design for both Glass Light and Glass Dark through tokens; do not maintain two unrelated fragments.
- Make the page responsive from phone width to wide desktop. Tables, code, charts, diagrams, and long identifiers must scroll or wrap inside their own region; the page itself must not acquire horizontal overflow.

## Content and security

- Markdown rendering can pass raw HTML through unchanged. Treat all inserted content as untrusted.
- A template must not attempt to sanitize, execute based on, or grant authority to inserted content. Glasspad's sandbox and response headers are the security boundary.
- Do not move inserted content into unsafe string-built HTML or evaluate it.
- Charts are available only through Glasspad's provided chart facility: a mount element plus `gp.chart(elementOrSelector, vegaLiteSpec)`. Do not bundle a chart library or fetch one. Use `--gp-chart-*` tokens when custom chart styling is needed.

## Markdown baseline and enhancements

A design must work with ordinary rendered Markdown: headings, paragraphs, links, lists, blockquotes, code, tables, and images. Optional classes may add richer layout or semantics, but they are progressive enhancements, not prerequisites. Document every new class and show what happens without it.

## Print

`prose` and `report` must print cleanly. `table` should preserve headers and avoid clipped columns where the browser can reasonably paginate them. For other areas, print should remain legible even if the screen composition collapses to a simple flow. Print-only hard-coded black/white values are acceptable when needed for reliable, ink-light output; screen colors remain token-native.
