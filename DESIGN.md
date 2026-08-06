# Glasspad Design System

> This is the design language glasspad's base stylesheet (`/_gp/v1/base.css`)
> implements. Glasspad hosts HTML artifacts the agent authors; a fragment
> artifact is wrapped in a themed shell that loads `base.css`, so these `--gp-*`
> tokens and component patterns are the vocabulary you author against — not the
> output of an automatic renderer. Full-document artifacts opt in by loading
> `base.css` themselves.

## 1. Visual Theme & Atmosphere

Glasspad is a precision instrument for data visualization — an AI-friendly scratchpad where dashboards, charts, tables, and rich content are shown with clarity and purpose. The design language draws from Linear's dark-mode precision, Notion's warm light-mode minimalism, and Vercel's typographic discipline.

The system ships two themes: **Glass Light** and **Glass Dark**. Light mode uses a soft warm-white canvas (`#fafbfc`) with near-black text (`#1a1b25`), creating a paper-like reading experience suited to data-dense dashboards. Dark mode inverts to a deep blue-black (`#0f1117`) where content emerges through calibrated luminance steps, ideal for extended working sessions.

Typography relies on the system font stack (`-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif`) for body text — fast to load, native-feeling, and optimized for data readability. Monospace content uses `'SF Mono', 'Fira Code', Menlo, Consolas, monospace` for code blocks and technical labels.

The color system is deliberately restrained: a single accent color (blue/indigo) handles all interactive elements, with the rest of the palette built from carefully calibrated neutrals. This ensures charts and data visualizations — the primary content — remain the visual focus, never competing with chrome.

**Key Characteristics:**
- Dual-theme: warm light (`#fafbfc`) and deep dark (`#0f1117`) with CSS custom properties
- System font stack for zero-latency native feel
- Single accent color per theme: blue (`#2563eb`) light / indigo (`#6366f1`) dark
- Whisper-weight borders: `1px solid` at low opacity for structure without noise
- Content-first: your artifact content is the hero — chrome is invisible
- Data-dense layouts with generous internal padding on cards
- Theme switching via `prefers-color-scheme` or explicit toggle with localStorage persistence

## 2. Color Palette & Roles

### Light Theme (Glass Light)

#### Backgrounds
- **Page Canvas** (`#fafbfc`): Primary page background — cool-warm white, not sterile
- **Surface** (`#ffffff`): Card backgrounds, elevated content areas
- **Surface Alt** (`#f3f4f6`): Stat cards, code blocks, subtle insets
- **Surface Hover** (`#f9fafb`): Table row hover, list item hover

#### Text
- **Primary** (`#1a1b25`): Headlines, primary content — near-black with slight warmth
- **Secondary** (`#374151`): Section headers, important labels
- **Muted** (`#6b7280`): Descriptions, metadata, secondary content
- **Faint** (`#9ca3af`): Timestamps, placeholders, least-emphasis text

#### Accent & Interactive
- **Accent** (`#2563eb`): Links, primary buttons, active indicators
- **Accent Hover** (`#1d4ed8`): Hover state for accent elements
- **Accent Soft** (`#eff6ff`): Filter bar background, accent-tinted surfaces
- **Accent Border** (`#bfdbfe`): Filter bar border, accent-tinted borders
- **Accent Tag** (`#dbeafe`): Filter tags, badges
- **Accent Text** (`#1e40af`): Text on accent-tinted backgrounds

#### Borders & Dividers
- **Border** (`#e5e7eb`): Standard card borders, table borders, dividers
- **Border Subtle** (`#f3f4f6`): Table row separators, inner dividers
- **Border Strong** (`#d1d5db`): Blockquote borders, emphasized dividers

#### Semantic
- **Error** (`#dc2626`): Error messages, danger states
- **Focus Ring** (`#3b82f6`): Keyboard focus indicators

### Dark Theme (Glass Dark)

#### Backgrounds
- **Page Canvas** (`#0f1117`): Deep blue-black page background
- **Surface** (`#1a1b26`): Card backgrounds — one luminance step up
- **Surface Alt** (`#242530`): Stat cards, code blocks, inset areas
- **Surface Hover** (`#2a2b38`): Table row hover, list item hover

#### Text
- **Primary** (`#e5e7eb`): Headlines, primary content — soft white, not harsh
- **Secondary** (`#9ca3af`): Section headers, labels
- **Muted** (`#6b7280`): Descriptions, metadata
- **Faint** (`#4b5563`): Timestamps, placeholders

#### Accent & Interactive
- **Accent** (`#6366f1`): Links, primary buttons, active indicators — indigo
- **Accent Hover** (`#818cf8`): Hover state for accent elements
- **Accent Soft** (`rgba(99, 102, 241, 0.1)`): Filter bar background
- **Accent Border** (`rgba(99, 102, 241, 0.25)`): Filter bar border
- **Accent Tag** (`rgba(99, 102, 241, 0.2)`): Filter tags, badges
- **Accent Text** (`#a5b4fc`): Text on accent-tinted backgrounds

#### Borders & Dividers
- **Border** (`rgba(255, 255, 255, 0.08)`): Standard borders — semi-transparent white
- **Border Subtle** (`rgba(255, 255, 255, 0.04)`): Inner dividers
- **Border Strong** (`rgba(255, 255, 255, 0.12)`): Emphasized dividers

#### Semantic
- **Error** (`#ef4444`): Error messages, danger states
- **Focus Ring** (`#6366f1`): Keyboard focus indicators

## 3. Typography Rules

### Font Family
- **Primary**: `-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif`
- **Monospace**: `'SF Mono', 'Fira Code', 'Fira Mono', Menlo, Consolas, monospace`

### Hierarchy

| Role | Size | Weight | Line Height | Letter Spacing | Notes |
|------|------|--------|-------------|----------------|-------|
| Page Title | 1.75rem (28px) | 700 | 1.25 | -0.02em | Dashboard title, single per page |
| Section Header | 0.95rem (15.2px) | 600 | 1.4 | 0.03em | Uppercase, card headers |
| Body | 0.9rem (14.4px) | 400 | 1.5 | normal | Standard text, table cells |
| Body Emphasis | 0.9rem | 600 | 1.5 | normal | Table headers, labels |
| Small | 0.85rem (13.6px) | 400 | 1.4 | normal | Buttons, links, captions |
| Caption | 0.8rem (12.8px) | 400-600 | 1.3 | 0.04em | Metadata, filter labels, TOC |
| Stat Value | 1.75rem (28px) | 700 | 1.2 | normal | Stats card numbers |
| Stat Label | 0.8rem | 400 | 1.3 | normal | Stats card descriptions |
| Code | 0.85rem | 400 | 1.5 | normal | Monospace, code blocks |

### Principles
- **Uppercase section headers**: All section `<h3>` headings use uppercase with letter-spacing `0.03em` — creates clear visual hierarchy without size inflation
- **Data-first sizing**: Body text at 0.9rem keeps tables and lists compact without sacrificing readability
- **Weight restraint**: Three weights only — 400 (read), 600 (label/emphasize), 700 (title/stat). No bold abuse
- **Monospace for data**: Stat values, code blocks, and technical content use the monospace stack

## 4. Component Stylings

### Section Cards
- Background: `var(--gp-surface)`
- Border: `1px solid var(--gp-border)`
- Radius: 12px
- Padding: 1.5rem
- Shadow (light): `0 1px 3px rgba(0,0,0,0.04)`
- Shadow (dark): `0 1px 3px rgba(0,0,0,0.2)`

### Stats Cards
- Background: `var(--gp-surface-alt)`
- Radius: 8px
- Padding: 1rem
- Layout: auto-fit grid, minmax(140px, 1fr)

### Tables
- Header: uppercase, 0.8rem, weight 600, muted text, 2px bottom border
- Rows: 0.55rem vertical padding, subtle bottom border
- Hover: `var(--gp-surface-hover)` background
- Sort indicators: transition opacity on hover/active

### Filter Bar
- Background: `var(--gp-accent-soft)`
- Border: `1px solid var(--gp-accent-border)`
- Radius: 10px
- Tags: pill-shaped (`border-radius: 999px`), accent-tinted

### List Items (Cards layout)
- Border: `1px solid var(--gp-border)`
- Radius: 8px
- Hover: accent-tinted border, subtle background shift
- Title: 0.9rem weight 600
- Subtitle/Meta: 0.8rem muted text

### Buttons & Controls
- Show More: `var(--gp-surface-alt)` background, `var(--gp-border)` border, 8px radius
- Collapse links: underline, muted text, darken on hover
- Focus: `2px solid var(--gp-focus)`, 2px offset

### Markdown Body (legacy dashboard inline-markdown)
> Describes inline markdown rendered *inside a dashboard card*. For the
> markdown render path the authoritative styling is the Prose / Reading Theme
> below (`.gp-prose`), not this section.
- Line height: 1.7 for comfortable reading
- Headings: h1/h2 get bottom border, progressive size reduction
- Code inline: accent-pink text (`#d63384` light / `#f472b6` dark), surface-alt background
- Code blocks: surface-alt background, border, 8px radius
- Blockquotes: strong border left, subtle background
- Tables: auto width, striped rows in light mode

### Prose / Reading Theme (`.gp-prose`)
A first-class reading layout beside the data-dashboard styles, for editorial /
long-form / markdown content. It is a **layout + typography variant**, not a
third color theme — it reads its colors from the same `--gp-*` tokens, so it
inherits Glass Light / Glass Dark automatically.

Opt in by wrapping content in an element carrying the `gp-prose` class
(`<article class="gp-prose"> … </article>`). All rules are scoped under that
class, so nothing changes for content that does not opt in. This is the default
template target for the markdown render path.

**Render contract:** rendered blocks are direct children of `.gp-prose` (the
first/last child margins are flushed on that assumption). The styles are built
to survive arbitrary markdown-generated HTML.

- **Column**: centered, `max-width: var(--gp-prose-measure)` (~720px reading measure); `min-width: 0` + `overflow-wrap` so long URLs / wide content never blow it out
- **Body**: `var(--gp-font-serif)` at `var(--gp-prose-font-size)` (~18px), line height 1.75
- **Headings**: same serif, sentence-case (no uppercase/tracking), generous space above / tight below; h1 large, h2 with a `--gp-border-strong` bottom rule
- **Links**: underlined (readability over the dashboard's clean look)
- **Lists**: logical (RTL-safe) indent; loose-list `<p>` gaps collapsed; GFM task-list checkboxes de-marked, aligned, and `accent-color`-themed
- **Blockquotes**: `--gp-border-strong` inline-start rule, secondary italic prose (italic reset on code / nested headings)
- **Tables**: sentence-case headers (not the dashboard uppercase signature); a wide table scrolls inside its own box rather than overflowing the column
- **Images**: inline by default (badges/icons keep flow); `<figure>` images and image-only paragraphs go block-centered with an 8px radius
- **Definition lists, sub/sup**: styled for the reading rhythm
- **Code**: inline/`pre`/`kbd`/`samp` reuse the base pink-on-alt treatment, sized relative to the reading body
- **Native controls**: `color-scheme` is bound per theme so checkboxes/scrollbars match Glass Light / Glass Dark
- **Print**: `@media print` forces a light, full-width flow and avoids splitting code/quotes/figures/tables across pages

Deferred to the markdown render feature (needs that renderer's HTML contract):
syntax-highlight color tokens (`--gp-syntax-*`) and the exact footnote-section
class vocabulary.

## 5. Layout Principles

### Spacing System
- Base unit: 8px (0.5rem)
- Scale: 0.25rem (4px), 0.5rem (8px), 0.75rem (12px), 1rem (16px), 1.5rem (24px), 2rem (32px)
- Card padding: 1.5rem (24px)
- Grid gap: 1.5rem (24px)
- Section header margin-bottom: 1rem

### Grid & Container
- Max content width: 1400px, centered
- Body padding: 2rem
- Grid layouts: 2-column (`repeat(2, 1fr)`) or 3-column (`repeat(3, 1fr)`)
- Stack layout: single column flex
- Span-full: cards can span entire grid width

### Whitespace Philosophy
- **Cards as containers**: Each section lives in its own card — the card border and padding create natural breathing room
- **Grid gap as rhythm**: The 1.5rem grid gap provides consistent inter-section spacing
- **Dense internals, spacious surroundings**: Tables and lists are compact inside cards, but cards float in generous whitespace

### Border Radius Scale
- Small (4px): Inline code, small badges
- Standard (6px): Inputs, small buttons
- Card (8px): Stat cards, list items, show-more buttons
- Section (12px): Section cards, primary containers
- Pill (999px): Filter tags, status badges

## 6. Depth & Elevation

| Level | Light Treatment | Dark Treatment | Use |
|-------|----------------|----------------|-----|
| Canvas | No shadow, `#fafbfc` bg | No shadow, `#0f1117` bg | Page background |
| Card | `0 1px 3px rgba(0,0,0,0.04)` + border | `0 1px 3px rgba(0,0,0,0.2)` + border | Section cards |
| Inset | `var(--gp-surface-alt)` bg, no shadow | `var(--gp-surface-alt)` bg, no shadow | Stat cards, code blocks |
| Hover | Background shift to `--gp-surface-hover` | Background shift to `--gp-surface-hover` | Table rows, list items |
| Focus | `2px solid var(--gp-focus)`, 2px offset | `2px solid var(--gp-focus)`, 2px offset | Keyboard navigation |

**Shadow Philosophy**: Glasspad uses minimal shadows — depth comes primarily from border containment and background color stepping. In light mode, a single subtle shadow (`0 1px 3px`) provides just enough lift for cards. In dark mode, shadows deepen slightly but the primary depth cue is luminance stepping: canvas (`#0f1117`) < surface (`#1a1b26`) < surface-alt (`#242530`).

### Collapsed Section Gradient
- Light: `linear-gradient(transparent, var(--gp-surface))`
- Dark: `linear-gradient(transparent, var(--gp-surface))`
- Both use the surface color to create a seamless fade-to-card effect

## 7. Do's and Don'ts

### Do
- Use CSS custom properties (`var(--gp-*)`) for all colors — never hardcode hex values in components
- Keep the accent color singular — one blue (light) or indigo (dark) for all interactive elements
- Use uppercase + letter-spacing for section headers — it's the signature hierarchy signal
- Maintain generous card padding (1.5rem) even when internal content is dense
- Use `var(--gp-surface)` for collapsed-section gradient endpoints
- Test all themes against every content type (charts, tables, stats, lists, markdown)
- Use semi-transparent borders in dark mode (`rgba(255,255,255,0.08)`) — solid dark borders look dead

### Don't
- Don't use pure white (`#ffffff`) as page background — use `#fafbfc` (light) or the surface color for cards only
- Don't use pure black (`#000000`) for text — use `#1a1b25` (light) or `#e5e7eb` (dark)
- Don't add heavy shadows — one subtle shadow per card maximum
- Don't use the accent color for non-interactive decorative elements
- Don't hardcode colors in JavaScript — read from CSS custom properties or use the Vega-Lite config object
- Don't use different border-radius values for the same component type across themes
- Don't add gradients or glassmorphism effects — Glasspad's depth is flat + border + luminance

## 8. Responsive Behavior

### Breakpoints
| Name | Width | Key Changes |
|------|-------|-------------|
| Mobile | <768px | Grid collapses to single column, TOC sidebar hidden, reduced padding |
| Desktop | >=768px | Full grid layout, TOC sidebar visible, standard padding |

### Touch Targets
- Buttons: minimum 0.6rem vertical padding, full-width show-more
- Filter tags: 0.25rem 0.6rem padding, pill shape for easy tapping
- Sort buttons: full column-header width for easy targeting
- Collapse links: generous hit area via padding

### Collapsing Strategy
- Grid layouts collapse to single column at 768px
- TOC sidebars (dashboard + markdown) hidden on mobile
- TOC margin offsets removed on mobile
- Body padding reduces from 2rem to 1rem

### Print
- All sidebars hidden
- All margin offsets removed
- Standard body flow

## 9. Agent Prompt Guide

### Quick Color Reference (Light)
- Page bg: `#fafbfc`
- Card bg: `#ffffff`
- Text: `#1a1b25`
- Muted text: `#6b7280`
- Accent: `#2563eb`
- Border: `#e5e7eb`

### Quick Color Reference (Dark)
- Page bg: `#0f1117`
- Card bg: `#1a1b26`
- Text: `#e5e7eb`
- Muted text: `#6b7280`
- Accent: `#6366f1`
- Border: `rgba(255,255,255,0.08)`

### CSS Custom Property Namespace
All theme tokens use the `--gp-` prefix:
- `--gp-bg`: Page canvas
- `--gp-surface`: Card/elevated surface
- `--gp-surface-alt`: Inset surface (stats, code)
- `--gp-surface-hover`: Hover state background
- `--gp-text`: Primary text
- `--gp-text-secondary`: Secondary text
- `--gp-text-muted`: Muted text
- `--gp-text-faint`: Faintest text
- `--gp-accent`: Interactive accent color
- `--gp-accent-hover`: Accent hover state
- `--gp-accent-soft`: Accent-tinted background
- `--gp-accent-border`: Accent-tinted border
- `--gp-accent-tag`: Accent-tinted tag/badge
- `--gp-accent-text`: Text on accent surfaces
- `--gp-border`: Standard border
- `--gp-border-subtle`: Subtle inner border
- `--gp-border-strong`: Emphasized border
- `--gp-error`: Error/danger text
- `--gp-focus`: Focus ring color
- `--gp-shadow`: Card shadow
- `--gp-code-text`: Inline code text color
- `--gp-chart-axis`: Chart axis tick/label color
- `--gp-chart-grid`: Chart gridline color

Typographic & layout tokens (theme-independent — identical in light and dark):
- `--gp-font-sans`: System UI stack — dashboard/body text
- `--gp-font-mono`: Monospace stack — code, technical labels
- `--gp-font-serif`: Reading serif stack — the prose theme
- `--gp-prose-measure`: Reading-column max width (`45rem` ≈ 720px)
- `--gp-prose-font-size`: Prose body size (`1.125rem` ≈ 18px)
- `--gp-prose-line-height`: Prose body line height (`1.75`)

### Theme Selection
- `auto` (default): follows `prefers-color-scheme` media query
- `light`: forces Glass Light theme
- `dark`: forces Glass Dark theme
- The wrapper resolves the theme (optional `theme` in a space's `glasspad.yaml`,
  else the user's toggle / system preference) and stamps it as the `data-theme`
  attribute on `<html>`; the toggle persists the choice in `localStorage`.
