---
created: 2026-04-10
updated: 2026-07-23
type: feature
reporter: maintainer
assignee: jari
status: obsolete
priority: normal
slug: kanban-board-view
closed: 2026-07-23
---

# Kanban board view

_Source: pad content types_

## Description

Add a Trello-style kanban board section type to Glasspad. Data is displayed as cards organized into vertical lanes (columns), with the ability to open a card to see its full details.

## UI Components

### Lanes (columns)
- Vertical columns representing a status, category, or grouping field
- Lane header with title and card count
- Scrollable when cards overflow vertically
- Horizontal scrolling when many lanes

### Cards
- Compact card showing key fields (title, labels, assignee, etc.)
- Configurable which fields appear on the card surface
- Visual indicators: color labels, priority badges, avatars
- Drag-and-drop reordering (future — ties into @bidirectional-actions bidirectional actions)

### Card detail view
- Click a card to open full detail panel (similar to list section detail view)
- Shows all fields of the item
- Support for rich content in description (markdown, HTML)
- Close with Escape or click outside

## Configuration

```yaml
sections:
  - title: "Project Board"
    type: kanban
    source: tasks
    kanban:
      lane_field: status          # field that determines which lane
      lanes:                      # explicit lane order and labels
        - key: todo
          label: "To Do"
        - key: in_progress
          label: "In Progress"
        - key: done
          label: "Done"
      card_title_field: title
      card_fields:                # fields shown on card surface
        - assignee
        - priority
        - labels
```

## Acceptance Criteria

- [ ] New `kanban` section type in backend (schema + validation)
- [ ] Lanes render as horizontal columns from data
- [ ] Cards display within their correct lane based on `lane_field`
- [ ] Clicking a card opens a detail view with all fields
- [ ] Responsive — horizontal scroll on narrow screens
- [ ] Works with cross-filtering from other sections
