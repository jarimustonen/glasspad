# 4. Board

## Audience

Project teams and people supervising agent or implementation work. They need a current view of what is done, next, blocked, and later, not a drag-and-drop planning application.

## Jobs to be done

- Scan progress and bottlenecks quickly.
- Distinguish states without relying on color alone.
- Understand dependencies, owners, and next actions.
- Follow links to the underlying work.

## Typical content

A title and update note; second-level status groups; task lists; owner or date metadata; compact status tables; blockquotes for risks; and sometimes an authored inline-SVG dependency diagram.

## Desired experience

Dense, stable, and operational. It may evoke columns or swimlanes, but source order and semantic headings must remain clear. Avoid mimicking an editable kanban product when the artifact is primarily a read-only status view.

## Plain-Markdown baseline

Use [`../samples/board.md`](../samples/board.md). Lists under headings must become a strong board even without custom wrappers. The included diagram follows Glasspad's existing `.gp-diagram` and `.gp-status-*` conventions.

## Optional enhancements

Classes may identify status groups, owner/date metadata, compact task cards, or a full-width dependency region. Document the status-to-class mapping. Every state must also be written in text.

## Responsive behavior

Columns may sit side by side only when there is enough width. On phones they stack in source order, with headings preserved. A wide dependency diagram scales or scrolls inside its figure.

## Light and dark

Use the supplied done/next/blocked/future token triplets. Keep soft fills, strokes, and labels readable in both themes and do not introduce competing status meanings.

## Print

Secondary but legible. Stack the groups, preserve written status, and avoid clipping diagrams or task text.

## Acceptance criteria

- Done, next, blocked, and future work can be found quickly and are named in text.
- Plain heading-plus-list Markdown produces a coherent board without required classes.
- The sample's dependency diagram and legend fit without page-level overflow.
- At 360 px, groups stack in source order and task metadata remains readable.
- Links and task checkboxes have visible keyboard focus/native state where applicable.
- Both themes maintain status distinction for common color-vision deficiencies through labels and structure.
