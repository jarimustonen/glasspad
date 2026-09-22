# Release readiness board

Updated 22 September 2026 · Owner: Delivery team · [Read the release criteria](./report.md)

## Done

- **Package metadata**: owner: Mina: license and source links verified
- **Upgrade notes**: owner: Elias: migration examples checked
- **Accessibility pass**: owner: Noor: keyboard path and names reviewed

## Next

- **Installer smoke test**: owner: Noor: due 23 September
- **Release notes**: owner: Mina: summarize user-visible changes
- **Support briefing**: owner: Elias: confirm escalation contacts

## Blocked

- **Regional mirror**: waiting for provider maintenance to finish
- **Signed archive**: signing identity renewal is under review

> The signed archive blocks publication. The regional mirror does not; it can follow in a patch update.

## Later

- Compare download telemetry after seven days
- Retire the old setup guide after one month
- Review whether to add an offline bundle

## Dependency view

<figure class="gp-diagram">
<svg viewBox="0 0 720 230" role="img" aria-label="Release dependency diagram. Metadata and accessibility are done. Installer test and release notes are next. Signed archive is blocked. Publish follows those tasks; telemetry review is future." style="--gp-diagram-min-width: 640px">
<defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path class="gp-edge-arrow" d="M0 0 L8 4 L0 8 z"/></marker></defs>
<path class="gp-edge" d="M160 55 H210" marker-end="url(#arrow)"/><path class="gp-edge" d="M160 165 H210" marker-end="url(#arrow)"/><path class="gp-edge" d="M350 55 H400" marker-end="url(#arrow)"/><path class="gp-edge" d="M350 165 H400" marker-end="url(#arrow)"/><path class="gp-edge" d="M540 110 H590" marker-end="url(#arrow)"/>
<g class="gp-status-done"><rect class="gp-node" x="20" y="35" width="140" height="40" rx="8"/><text class="gp-node-label" x="90" y="60" text-anchor="middle">Metadata: done</text></g>
<g class="gp-status-done"><rect class="gp-node" x="20" y="145" width="140" height="40" rx="8"/><text class="gp-node-label" x="90" y="170" text-anchor="middle">Access: done</text></g>
<g class="gp-status-next"><rect class="gp-node" x="210" y="35" width="140" height="40" rx="8"/><text class="gp-node-label" x="280" y="60" text-anchor="middle">Installer: next</text></g>
<g class="gp-status-next"><rect class="gp-node" x="210" y="145" width="140" height="40" rx="8"/><text class="gp-node-label" x="280" y="170" text-anchor="middle">Notes: next</text></g>
<g class="gp-status-blocked"><rect class="gp-node" x="400" y="90" width="140" height="40" rx="8"/><text class="gp-node-label" x="470" y="115" text-anchor="middle">Signing: blocked</text></g>
<g class="gp-status-future"><rect class="gp-node" x="590" y="90" width="110" height="40" rx="8"/><text class="gp-node-label" x="645" y="115" text-anchor="middle">Publish: future</text></g>
</svg>
<ul class="gp-legend"><li class="gp-chip gp-status-done">Done</li><li class="gp-chip gp-status-next">Next</li><li class="gp-chip gp-status-blocked">Blocked</li><li class="gp-chip gp-status-future">Future</li></ul>
<figcaption>Critical release path; every state is repeated in text.</figcaption>
</figure>
