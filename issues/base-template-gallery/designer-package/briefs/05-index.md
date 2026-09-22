# 5. Index

## Audience

A person arriving at a multi-page Glasspad space who needs to understand what is available and choose the right page.

## Jobs to be done

- Understand the space's purpose at a glance.
- Find a page by task or topic rather than filename.
- Distinguish primary destinations from supporting references.
- See freshness, ownership, or status when supplied.

## Typical content

A title and short introduction; groups of links under headings; one-sentence descriptions; occasional update notes, status labels, and a small “start here” callout.

## Desired experience

A clean directory rather than a marketing landing page. Links should feel like navigable destinations, with clear descriptions and generous targets, while remaining recognizable as links.

## Plain-Markdown baseline

Use [`../samples/index.md`](../samples/index.md). Its headings and ordinary Markdown link lists must produce the complete navigation experience; do not require bespoke card markup or infer inaccessible information from URL shapes.

## Optional enhancements

Classes may mark a featured destination, compact metadata, a status chip, or a link-card list. They must not be necessary for navigation and must preserve valid anchors and source order.

## Responsive behavior

A link-card grid can expand on wide screens and becomes one column on phones. Long destination names and descriptions wrap; focus states are never clipped.

## Light and dark

Cards and groups use normal surfaces and borders. Accent is reserved for actual destinations and focus, not decorative blocks. Muted descriptions remain readable in dark mode.

## Print

Not a primary use, but printed links and their descriptions should remain legible. Showing URLs is optional if it can be done without clutter.

## Acceptance criteria

- A first-time visitor can identify the space and choose among its main destinations quickly.
- Every destination remains a normal, accessible link in the underlying content.
- The sample's grouped lists look complete without optional classes.
- Hover and keyboard focus reinforce navigation without moving the layout.
- Long text does not overflow at 360 px; wide layouts avoid sparse, oversized cards.
- Both themes preserve clear group and destination hierarchy.
