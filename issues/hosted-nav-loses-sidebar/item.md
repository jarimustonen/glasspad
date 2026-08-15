---
created: 2026-08-14
updated: 2026-08-15
type: bug
reporter: jari
status: cannot-reproduce
priority: normal
closed: 2026-08-15
---

# Hosted: navigating to a page can lose the grouped sidebar (observed on maalla.dev, 0.10.0 client)

_Source: aggountant project-view_

## Description

**Observed:** a space published to https://glasspad.maalla.dev (client 0.10.0) shows the grouped sidebar on the home page, but clicking a nav entry (e.g. the `latest` page) lands on the page **without the sidebar**. The served HTML for that page DOES contain the sidebar (`curl .../latest` shows `gp-sidebar`), and the same space served via local `glasspad loopback serve` (0.10.0) renders the sidebar correctly on every page — so this looks specific to the **hosted maalla.dev server**, which appears to run a build behind the 0.10.0 client (it also renders the shell chrome less refined than local). **Likely fix:** upgrade the hosted maalla.dev glasspad to match the client; verify the shell wraps every page URL, not just the home. Needs in-browser confirmation of the exact failing interaction. Tracked in aggountant `project-view`.

## Investigation (2026-08-15) — no code defect in current `main`; deploy/ops action required

**Conclusion: not reproducible as a code bug in current `main` (0.11.0). The
symptom is a deployed-version / stored-metadata mismatch on maalla.dev, not a
defect in the shell/space rendering.** A regression test was added to lock in the
correct behaviour so a future change can't silently reintroduce it.

**Why the current code cannot exhibit this:**

- The grouped sidebar is **trusted shell chrome**, not artifact content. It is
  built client-side from the `GROUPS` data literal the shell embeds
  (`shell::render_with_groups`), and `render_shell` (`src/artifact_host/mod.rs`)
  reads `snap.space(space).nav_groups` **per space, identically for every page
  slug** — the home landing and every non-home page get the same `GROUPS` literal.
- The reported interaction (clicking a nav entry) is an **in-place iframe swap**
  (`navigateTo(slug)` in the shell) — the shell document, including its sidebar,
  never reloads, so an in-place swap *cannot* drop the sidebar in current code.
- Even a **full-page load** of a non-home page URL (`/p/<space>/<page>`, e.g.
  open-in-new-tab) renders the same grouped sidebar, because the shell is rebuilt
  from the same per-space `nav_groups`.
- `nav_groups` **persists across a server restart**: the hosted store writes it to
  `meta.json` and re-validates it through `build_space_bundle` on reload
  (`src/hosted/store.rs`), so a hosted reboot keeps the grouped sidebar.

**Regression coverage added** (proves the above, would have caught a real
regression): `src/hosted/mod.rs::grouped_space_keeps_the_sidebar_on_every_page_url_and_across_reload`
— publishes a grouped space via the hosted ingest and asserts the `GROUPS` literal
(both group labels) is present on the home shell **and** every non-home page
shell, **and** again after a fresh `Store::open` on the same root (the reboot
path). Passes on current `main`.

**The `curl` observation is a red herring:** the shell HTML *always* contains an
empty `<aside class="gp-sidebar" id="gp-sidebar">` container (CSS hides it when
empty: `aside.gp-sidebar:empty { display:none }`). So `curl … | grep gp-sidebar`
matches even when the sidebar is unpopulated. The sidebar is only *populated*
client-side when the `GROUPS` literal is non-empty. The "sidebar on the home page"
the reporter saw was most likely the **generated landing page's own TOC** (an
artifact fragment inside the iframe, home-only by nature), while the shell-chrome
sidebar was never populated because the hosted `GROUPS` was empty.

**Root cause on maalla.dev — one of (both fixed by the same ops action):**

1. The hosted **glasspad server** predates 0.8.0 (grouped nav + generated landing
   landed in **v0.8.0**, commit `366a0cf`) — its ingest has no `groups` field, so
   a published space's `nav_groups` is dropped at ingest and the shell-chrome
   sidebar is always empty. The "shell chrome less refined than local" note in the
   report corroborates an older server build.
2. And/or the space's **stored `meta.json` has empty `nav_groups`** because it was
   published while the server was pre-0.8.0 — in which case upgrading the server is
   necessary but **not sufficient**; the space must be **re-published** with a
   current client against the upgraded server so `nav_groups` is persisted.

### Required deploy / ops action (outside this repo's code)

1. **Upgrade the hosted glasspad on maalla.dev to the current release (≥ 0.11.0).**
   `/healthz` returns only `ok` (no version), so verify the running build another
   way — see the check below.
2. **Re-publish the affected space** with a current (≥ 0.8.0) client so the server
   accepts + persists `groups`/`nav_groups`.
3. **Verify:** `curl -s https://glasspad.maalla.dev/p/<space>/<non-home-page>` and
   confirm the shell embeds a **non-empty `var GROUPS = […]`** literal carrying the
   group labels (not merely the empty `gp-sidebar` container). Equivalently,
   in-browser, a non-home page must show the grouped sidebar chrome. If `GROUPS` is
   `[]` on a non-home page after the upgrade, step 2 (re-publish) is still pending.

No terminal maalla.dev deploy is performed from this repo (there is no remote
deploy in this project — see CLAUDE.md "Deploy = localhost"). The ops action above
is the remaining work; it is tracked for the reporter in aggountant `project-view`.
