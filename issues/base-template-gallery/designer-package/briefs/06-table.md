# 6. Table

## Audience

Analysts, operators, buyers, and reviewers comparing many records or attributes. Their primary task is scanning, matching rows, and checking precise values.

## Jobs to be done

- Keep column meaning visible while moving through a long table.
- Compare values across rows with minimal visual noise.
- Read notes, units, and data freshness around the table.
- Use the page on a phone without breaking the whole layout.

## Typical content

A title, short data note, one dominant wide table, sometimes a second compact table, links in cells, code-like identifiers, statuses, units, and caveats below.

## Desired experience

Dense and trustworthy, with alignment and legibility taking priority over cards. The surrounding prose should orient the reader, then get out of the table's way.

## Plain-Markdown baseline

Use [`../samples/table.md`](../samples/table.md). A normal GFM table is the core input. Do not require producers to add wrappers around a Markdown table or duplicate headers.

## Optional enhancements

You may propose classes for numeric alignment, a sticky first column, compact density, status cells, or a table note. These are enhancements only. Do not imply sorting or filtering unless the returned fragment actually and accessibly provides it.

## Responsive behavior

Use available desktop width. Keep headers visible with `position: sticky` where the sandboxed scroll geometry allows it. On narrow screens, the table scrolls inside a clearly signaled local region; the body never scrolls sideways. Do not turn cells into unlabeled cards.

## Light and dark

Favor fine separators, restrained row hover, and stable contrast. Zebra treatment, if used, must be subtle in both themes. Links and statuses remain recognizable.

## Print

Important. Avoid clipping silently; repeat table headers where supported, reduce density carefully, and permit landscape-friendly width. Notes and units must print with the data.

## Acceptance criteria

- The sample's 10-column table is scannable at desktop width and locally scrollable at 360 px.
- Header-to-cell association remains semantically intact; no visual reordering changes meaning.
- Sticky behavior does not cover captions, headings, or focused links.
- IDs, numbers, dates, links, and multiline notes each receive suitable alignment/wrapping.
- Both themes show row boundaries and hover/focus without excessive striping.
- Print/PDF output includes all columns or makes any unavoidable continuation explicit rather than clipping silently.
