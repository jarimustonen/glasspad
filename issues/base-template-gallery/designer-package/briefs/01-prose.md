# 1. Prose

## Audience

People reading an AI-produced memo, analysis, article, decision record, or documentation page. They may read closely for several minutes and need to quote, print, or follow links.

## Jobs to be done

- Understand a structured argument without visual fatigue.
- Scan headings, then settle into a comfortable reading rhythm.
- Read code, tables, quotations, notes, and images without leaving the document.
- Navigate a longer page through Glasspad's existing optional table-of-contents rail.

## Typical content

One title, short context or metadata, multiple heading levels, paragraphs, links, ordered and unordered lists, blockquotes, fenced code, a modest table, and occasional images or captions.

## Desired experience

Editorial and calm, with stronger typographic craft than the current baseline but no magazine theatrics. Preserve the familiar centered reading measure and make supporting material feel integrated rather than bolted on.

## Plain-Markdown baseline

The complete design must work with ordinary Markdown blocks and no special classes. Use [`../samples/prose.md`](../samples/prose.md) as the primary input. Glasspad may generate a `.gp-toc` sibling beside the `.gp-prose` article when a page has at least two second- or third-level headings.

## Optional enhancements

You may propose classes for a compact document byline, lead paragraph, callout, or full-bleed figure. They must be optional and degrade to normal paragraphs, quotes, or figures. Do not make metadata extraction a prerequisite.

## Responsive behavior

Keep a comfortable measure on wide screens. Hide or relocate the TOC rail when it no longer fits. Long URLs and code wrap or scroll locally; tables scroll in their own region; images never exceed the column.

## Light and dark

Both themes should feel paper-like without using literal paper textures. Maintain readable long-form contrast, visible links, restrained rules, and native control colors through tokens.

## Print

Required. Remove navigation rails and decorative elevation, use a full printable width, keep links legible, and avoid splitting code, quotes, figures, and small tables where practical.

## Acceptance criteria

- The fragment uses `.gp-prose`, with the single content marker as its direct child.
- A 1,000 to 2,000 word page remains comfortable and clearly structured.
- Headings, lists, quotes, code, tables, images, and links in the sample all look intentional.
- The existing TOC sibling can sit beside the article without being nested or obstructed.
- No viewport-wide horizontal overflow occurs at 360 px.
- Light, dark, and print previews retain clear hierarchy.
