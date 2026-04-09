---
created: 2026-04-09
updated: 2026-04-09
type: epic
owner: jari
status: open
priority: normal
---

# E03. MCP integration

## Goal

Expose glasspad functionality as an MCP server so Claude Code and other MCP clients can create, manage, and interact with pads natively.

## Phases

### Phase 1: Core tools
- [ ] 11.1 MCP server: create_pad, update_pad, list_pads, delete_pad
- [ ] 11.2 MCP: wait_for_completion (blocking tool)

### Phase 2: Validation
- [ ] 11.3 Test in Claude Code environment

## Notes

Depends on E01 (bidirectional actions) for wait_for_completion support.
