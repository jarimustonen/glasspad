# 2. Dashboard

## Audience

Operators, project leads, and decision-makers checking a compact snapshot of metrics, trends, and exceptions. Most visits are brief and goal-directed.

## Jobs to be done

- Read the headline state in seconds.
- Compare a small set of KPIs and spot movement or exceptions.
- Inspect charts and a supporting table without losing context.
- Understand freshness and caveats.

## Typical content

A title and update line; short label/value metrics; second-level sections; one or two charts; a compact table; status notes and links to detail.

## Desired experience

A real dashboard rather than today's single white card: crisp hierarchy, efficient space, and a grid that feels composed with plain Markdown. Charts and numbers should lead; chrome should recede.

## Plain-Markdown baseline

The page must remain useful when rendered from headings, paragraphs, lists, and tables alone. Use [`../samples/dashboard.md`](../samples/dashboard.md). Do not assume the producer can wrap each Markdown section in custom markup or that every `h2` has the same content shape.

## Optional enhancements

Classes may opt specific blocks into metric tiles, card spans, alert emphasis, or chart regions. The sample includes a chart through `gp.chart(...)`; without the script, its heading and surrounding explanation must still communicate the topic.

## Responsive behavior

Use an auto-fitting grid at wide widths and a coherent single column on phones. Preserve reading order in the DOM. Chart mounts and tables stay inside their cards and never force page overflow.

## Light and dark

Use surface steps and fine borders rather than heavy shadows. Charts should read token values for axes, grid, background, and categorical colors. KPI emphasis must retain sufficient contrast in both themes.

## Print

Printing is secondary, but the page should collapse to a legible linear summary with no clipped cards or charts.

## Acceptance criteria

- The first viewport clearly answers “What is the state, and when was it updated?”
- Plain Markdown sections form a usable composition without required enhancement classes.
- Four headline metrics, a chart, notes, and the support table fit a desktop rhythm without crowding.
- At 360 px, cards follow source order and all wide content has local overflow handling.
- Both themes distinguish canvas, cards, insets, links, and exceptions without a new palette.
- Keyboard focus remains visible on every link.
