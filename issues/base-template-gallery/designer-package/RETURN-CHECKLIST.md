# Return checklist

Return one clearly named folder per template area: `prose`, `dashboard`, `report`, `board`, `index`, and `table`.

For **each** template, include:

- [ ] One self-contained `.html` fragment, not a complete HTML document.
- [ ] Exactly one literal `{{content}}` marker.
- [ ] A short usage note: intended content, strengths, and known limits.
- [ ] A light-theme preview made with the matching supplied sample.
- [ ] A dark-theme preview made with the matching supplied sample.
- [ ] A short rationale explaining hierarchy, layout, and key responsive choices.
- [ ] A list of optional enhancement classes, if any, with their meaning and a plain-Markdown fallback. Write “None” if there are none.
- [ ] Phone-width and desktop-width checks.
- [ ] An overflow check with the sample's table, code, image, chart, diagram, or long identifier as applicable.
- [ ] A keyboard-focus check for links and controls.
- [ ] A print check where the brief calls for one.

Across the full return:

- [ ] All screen colors and system styling use the supplied `--gp-*` tokens.
- [ ] Both themes use the same fragment rather than separate implementations.
- [ ] Fonts are system fonts; there are no external assets, imports, CDNs, or network calls.
- [ ] Plain rendered Markdown remains useful without optional classes or JavaScript.
- [ ] Status is not communicated by color alone.
- [ ] Charts use only the provided `gp.chart(...)` facility.
- [ ] Any `.gp-prose` wrapper has rendered blocks as direct children.
- [ ] The template does not attempt to sanitize or trust inserted raw HTML.

Glasspad's team will validate, adapt if needed, integrate the selected fragments into Rust, add product tests, and decide release sequencing. Rust integration is not part of the designer's assignment.
