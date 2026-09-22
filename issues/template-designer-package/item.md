---
created: 2026-09-22
updated: 2026-09-22
type: task
reporter: jari
status: done
priority: normal
related: ['@base-template-gallery']
lane: template-rendering
lane_seq: -50
closed: 2026-09-22
closed_by: agent
---

# Template designer package

## Goal

Create a self-contained handoff package that Jari can give to an external visual/product designer for the approved Glasspad built-in template gallery.

## Product decision

The approved gallery areas, in priority order, are: `prose`, `dashboard`, `report`, `board`, `index`, and `table`. This approves the product needs and sequence, not final visuals or a requirement to ship every template in one release.

## Deliverable

Under `issues/base-template-gallery/`, assemble a clearly named designer package containing:

- a short start-here brief written for a designer unfamiliar with Glasspad;
- the six template briefs, each with audience, typical use, desired experience, content shapes, responsive and light/dark expectations, and concrete acceptance criteria;
- the visual language and hard platform constraints distilled from the repository without requiring the designer to inspect source code;
- representative Markdown inputs for every template area, using realistic neutral content rather than lorem ipsum;
- a current-state baseline for the existing `prose` and `dashboard` templates in both themes, preferably browser-rendered visual references where the repository tooling can produce them reliably;
- an explicit expected-return checklist: one self-contained HTML fragment per template, usage notes, both-theme previews, and any optional enhancement classes;
- a manifest or index listing every file so the package can be copied or archived without hidden dependencies.

The package must distinguish product/design decisions from implementation details. Do not ask the designer to change Rust code or Glasspad integration. Preserve the platform contracts documented in `issues/base-template-gallery/design.md`.

## Done criteria

- The package is understandable and usable outside this repository.
- All six approved areas are covered consistently.
- Example content exercises headings, prose, lists, tables, code, images/placeholders, links, status data, and charts where relevant.
- Technical constraints are concrete but concise: fragment-only, exactly one `{{content}}`, token-native, dual-theme, no external network/font dependencies, responsive/overflow-safe, CSP/sandbox reality.
- Any images committed to the issue follow repository image-format policy.
- Relevant docs/links are either included in distilled form or clearly identified as optional source material; no personal machine paths or internal-only assumptions appear.
- Validate package links and examples, perform a proportionate focused self-review, and commit the result.

## Relationship

This is the approved preparation stage of `base-template-gallery`. Completing this task does not close the parent gallery feature; external design and later product integration remain in that issue.

## Decisions

### 2026-09-22T14:03:43Z · @jari

Final delivery must also be packaged as a standalone ZIP archive and copied to the Downloads folder on the hauis machine. The archive must contain only the designer handoff package, open cleanly without repository context, and use a descriptive stable filename.
