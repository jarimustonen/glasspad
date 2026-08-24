# Markdown preprocessing

Use producer-side preprocessing when a markdown space needs document semantics beyond rendering.

#### Producer preprocessing: glossary autolinks and cross-references

Glasspad deliberately does not infer document semantics. If a space needs glossary-term
autolinking, cross-reference resolution, citations, or similar transformations, preprocess
the markdown in the producer's build step and give glasspad the resulting space. Keep the
source and generated directories separate:

```text
docs-src/                         build/docs/
  guide.md          preprocessor   guide.md
  glossary.md       ───────────>   glossary.md
                                  glasspad.yaml
                                  templates/prose.html
```

For example, a producer can define `{{glossary:ledger|Ledger}}` as its own source notation
and turn this:

```markdown
A {{glossary:ledger|Ledger}} records the entries. See [Posting rules](posting.md).
```

into ordinary markdown plus an HTML link when a semantic class is needed:

```markdown
A <a class="xref glossary-term" href="glossary.html#ledger">Ledger</a> records the
entries. See [Posting rules](posting.md).
```

Then publish only the generated directory:

```bash
python build_docs.py docs-src build/docs

glasspad publish build/docs
```

Use a Markdown parser or another structure-aware transform in a real autolinker so it does
not rewrite terms inside code, existing links, or HTML. Keep link targets path-relative
(`posting.md` or `glossary.html#ledger`) for normal same-space navigation.

The renderer's class contract is intentionally simple. CommonMark links such as
`[Posting rules](posting.md)` render as plain `<a href="…">` elements and have no class
syntax. Raw HTML in markdown is passed through verbatim, without a sanitizer, so all
classes authored on an HTML link survive into the prose artifact. This is not a privileged
allowlist: the whole artifact remains untrusted inside the existing null-origin sandbox.
A producer can therefore style semantic links in a space template:

```yaml
# build/docs/glasspad.yaml
template: templates/prose.html
```

```html
<!-- build/docs/templates/prose.html -->
<style>
  .gp-prose a.xref { text-decoration-style: dotted; }
  .gp-prose a.glossary-term::after { content: " · glossary"; font-size: 0.75em; }
</style>
<article class="gp-prose">{{content}}</article>
```

The custom template remains an artifact fragment and may style against the `--gp-*`
tokens. Preprocessing decides which words and destinations have meaning; glasspad only
renders and hosts the links.
