---
created: 2026-08-15
updated: 2026-08-16
type: improvement
status: wontfix
priority: low
related: ['@hosted-store-generation-pointer']
closed: 2026-08-16
---

# Generation-pointer auto-heal from a genuinely-lost current pointer (durable previous-pointer / sequence)

## Description

From the hosted-store-generation-pointer review panel. Today, if a space/live `current` pointer is genuinely lost after an Unconfirmed commit + crash (portable-FS edge; won't happen on ext4 default where the rename rolls back to the prior pointer), recovery conservatively PRESERVES all generations but the unit is dark (served nothing) until an operator or the next durable write heals it — no data loss, but an availability gap. To auto-heal, add either a durable two-slot pointer (current-a/current-b, alternating, never overwrite the only known-good slot) or a monotonic sequence in each generation so recovery can pick the newest complete generation when the pointer is unresolvable. Also fold in legacy-migration cleanup (remove stale top-level flat meta/artifacts after the first confirmed generation commit) and MAX_PAGES enforcement on scan/load. Raised by openai (structural), gemini, deepseek.
