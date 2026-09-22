# Current-state baseline

These references capture Glasspad 0.17.4's two built-in templates before the gallery design work. They are evidence of the starting point, not visual requirements.

## What exists today

- `prose` is a centered `.gp-prose` reading column. Glasspad can add an “On this page” rail for documents with enough section headings.
- `dashboard` is only one `.gp-card` around the whole rendered Markdown document. It has no metric grid or section composition yet.

The exact fragments are included at [`../reference/current-prose.html`](../reference/current-prose.html) and [`../reference/current-dashboard.html`](../reference/current-dashboard.html). The exact shipped stylesheet is [`../reference/base.css`](../reference/base.css).

## Browser-rendered captures

| Template | Light | Dark |
|---|---|---|
| Prose | [prose-light.avif](prose-light.avif) | [prose-dark.avif](prose-dark.avif) |
| Dashboard | [dashboard-light.avif](dashboard-light.avif) | [dashboard-dark.avif](dashboard-dark.avif) |

The captures use a 1440 × 1100 browser viewport and the packaged source inputs in [`source/`](source/). They show the top of each page; the complete Markdown source is included for inspection. AVIF conversion may preserve a few pixels outside the viewport crop depending on encoder alignment.

## Reproduce

With Glasspad 0.17.4 and a Chromium-family browser installed:

```sh
glasspad loopback serve source/prose.md --template prose --port 43127
# Open http://127.0.0.1:43127/prose/

glasspad loopback serve source/dashboard.md --template dashboard --port 43128
# Open http://127.0.0.1:43128/dashboard/
```

Select Glass Light or Glass Dark using the trusted viewer's theme control, then capture the artifact frame at 1440 × 1100. The supplied captures were produced with headless Google Chrome against the local loopback server. No hosted service or external content was used. The dashboard sample contains a chart call, but the baseline viewport is still meaningful if chart execution is unavailable: today the template itself supplies only the outer card.

## Known limitation

The preferred macOS browser helper could not run on the Linux capture host because `osascript` was unavailable. Its required first error check was attempted and failed for that environmental reason. The local page was instead rendered directly in headless Chrome; the files were checked as valid AVIF images and the local HTTP responses were verified.
