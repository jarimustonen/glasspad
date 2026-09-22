# Glasspad template gallery: designer handoff

## Start here

Glasspad is a lightweight way for AI agents to turn Markdown or HTML into a polished page and share it with a person. The page may be a memo, dashboard, report, project view, directory, or data table. It appears inside Glasspad's secure viewer and automatically follows the reader's light or dark theme.

A **template** is one HTML fragment that surrounds rendered Markdown. It supplies layout and presentation, not content. Glasspad replaces its single `{{content}}` marker with the rendered Markdown and supplies the page shell, theme tokens, and chart library.

We are asking you to design six built-in template directions, in this priority order:

1. `prose`: long-form reading
2. `dashboard`: metrics and charts
3. `report`: narrative business or research deliverables
4. `board`: status and progress views
5. `index`: a space's navigation front page
6. `table`: data-first pages

The areas and their order are approved product needs. The final visual treatment is yours to propose, and the six designs do not have to ship in one release.

## What to return

For each area, return:

- one self-contained HTML fragment containing exactly one `{{content}}` marker;
- a short usage note;
- a light-theme preview and a dark-theme preview;
- a list of any optional enhancement classes you introduce; and
- a short design rationale.

See [RETURN-CHECKLIST.md](RETURN-CHECKLIST.md) for the complete handoff checklist. You do **not** need to write Rust, wire templates into Glasspad, or package a release.

## Suggested workflow

1. Read the [technical contract](reference/technical-contract.md). These are hard platform boundaries.
2. Read the [visual-language guide](reference/visual-language.md).
3. Review the [current-state baseline](baseline/README.md). It shows what exists today, not a target to copy.
4. Work through the six briefs in `briefs/`, using the matching realistic input in `samples/`.
5. Check each result against the brief's acceptance criteria and the [return checklist](RETURN-CHECKLIST.md).

## Package map

- [`briefs/`](briefs/): one consistent product brief per approved area
- [`samples/`](samples/): realistic Markdown inputs to design against
- [`reference/`](reference/): visual language, platform contract, shipped CSS, and current fragment references
- [`baseline/`](baseline/): reproducible current `prose` and `dashboard` references in both themes
- [`assets/`](assets/): local placeholder artwork used by the samples
- [`MANIFEST.md`](MANIFEST.md): every packaged file and its role

Everything linked from this package is included here. It has no dependency on a repository, private service, personal path, CDN, or external font.
