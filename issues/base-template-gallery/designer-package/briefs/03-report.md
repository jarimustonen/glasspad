# 3. Report

## Audience

A client, manager, researcher, or colleague receiving a finished business or research deliverable. They may read on screen, share a PDF, or print it for discussion.

## Jobs to be done

- Identify the report, date, scope, and conclusion quickly.
- Read narrative evidence alongside charts and tables.
- Distinguish findings, recommendations, limitations, and source notes.
- Produce a credible print or PDF version without redesign.

## Typical content

A first-level title followed by date/author context, an executive summary, numbered findings, charts, data tables, quotations, methodology or limitations, recommendations, and references.

## Desired experience

A polished hybrid between editorial prose and analytical data. It should feel complete enough to hand to another person, but neutral enough for varied organizations and topics.

## Plain-Markdown baseline

Use [`../samples/report.md`](../samples/report.md). The first `h1` and following paragraph should read as a title block through styling alone; the template must not depend on extracting or moving those nodes. Every section remains understandable as normal Markdown.

## Optional enhancements

You may propose classes for an executive summary, finding, recommendation, source note, figure, or wide table. Each must have a sensible ordinary-Markdown fallback. Charts use only the supplied `gp.chart(...)` facility.

## Responsive behavior

Keep narrative at a readable measure while allowing figures and tables more width where possible. On phones, all sections become one flow and wide elements scroll locally.

## Light and dark

The screen version must be equally credible in both themes. Avoid report-specific hard-coded brand colors. Charts, rules, captions, and metadata use tokens.

## Print

Essential. Create an ink-light white-page treatment, sensible title hierarchy and margins, repeated table headers where supported, and practical break rules for headings, figures, code, quotes, and compact tables. Hide screen-only controls.

## Acceptance criteria

- The title and context read as a deliberate cover/header without moving content in script.
- Executive summary, findings, chart, table, recommendation, limitation, and sources have distinct but coherent hierarchy.
- Narrative line length remains comfortable while data can use available width.
- Both 360 px and wide desktop layouts avoid page-level horizontal overflow.
- Light and dark screen previews and a print/PDF preview are supplied.
- The printed result remains intelligible in grayscale and avoids wasteful dark backgrounds.
