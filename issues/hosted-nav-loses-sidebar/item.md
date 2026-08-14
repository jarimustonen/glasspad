---
created: 2026-08-14
updated: 2026-08-14
type: bug
reporter: jari
status: open
priority: normal
---

# Hosted: navigating to a page can lose the grouped sidebar (observed on maalla.dev, 0.10.0 client)

_Source: aggountant project-view_

## Description

**Observed:** a space published to https://glasspad.maalla.dev (client 0.10.0) shows the grouped sidebar on the home page, but clicking a nav entry (e.g. the `latest` page) lands on the page **without the sidebar**. The served HTML for that page DOES contain the sidebar (`curl .../latest` shows `gp-sidebar`), and the same space served via local `glasspad loopback serve` (0.10.0) renders the sidebar correctly on every page — so this looks specific to the **hosted maalla.dev server**, which appears to run a build behind the 0.10.0 client (it also renders the shell chrome less refined than local). **Likely fix:** upgrade the hosted maalla.dev glasspad to match the client; verify the shell wraps every page URL, not just the home. Needs in-browser confirmation of the exact failing interaction. Tracked in aggountant `project-view`.
