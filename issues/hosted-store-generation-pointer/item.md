---
created: 2026-08-15
updated: 2026-08-15
type: improvement
status: done
priority: normal
closed: 2026-08-15
---

# Hosted store: immutable generation dirs + current-pointer for live overlay & space/key durability

## Description

## Description

Immutable **generation directories** + an atomically-swapped **current-generation pointer** for the hosted store's mutable-content paths, replacing the in-place staged-replace + two-file overlay protocols. Surfaced by the `materialize-space-durability` review panel (gpt-5.6-sol, deepseek-v4-pro; opus concurring) as findings F13 + F14 — both resolve to the same architecture (the issue's original "option 2").

## Context

`materialize-space-durability` closed the fsync-after-swap **divergence** (memory vs disk) and made durability honest across `materialize_space` / `write_page` / `write_live` via a `Committed{Durable|Unconfirmed}` outcome (commit c522e88). It deliberately did NOT do the deeper generation-pointer redesign — left here as the tracked follow-up.

Two residual, pre-existing crash-consistency weaknesses remain, both fixed by generations + a pointer:

- **F13 — `write_live` two-file protocol can lose a committed round.** `live.html` is renamed before `live.json`; a crash between them (or a failed re-push over an existing overlay) yields a digest-mismatched pair that load discards, reverting to the immutable baseline and losing the previously-committed round N. An in-place overwrite cannot preserve the old generation.

- **F14 — space + stable-key mapping is not atomic across a crash.** Even with swap-then-surface + skip-mapping-on-unconfirmed, full exactly-once semantics for `--space-key` re-publish across a power loss need a fsync'd transaction spanning both the space generation and the key mapping, or a generation pointer that flips both atomically.

## Proposed shape

```
spaces/<slug>/
  generations/<gen-id>/   # immutable: meta.json + artifacts/ + assets/
  current                 # atomically-replaced pointer/manifest -> <gen-id>
```

Protocol: write + fsync a complete new generation → fsync the generation dir → atomically replace `current` (single rename) → fsync parent → retain the prior generation until the commit's durability is confirmed. Recovery reads `current`; GC reaps non-current generations. Apply the same immutable-generation + pointer model to the live overlay so round N survives a failed round N+1.

## Also fold in (same review, deferred)

- Route the post-commit durability failure to structured logging/metrics (currently `eprintln!` + a surfaced 500) — F9, deferred until the project grows a tracing/metrics layer.
- Optional `renameat2(RENAME_EXCHANGE)` on Linux to make the replace atomic for external filesystem observers, not just the in-memory serving snapshot — F12 (doc-only today).

## Out of scope / not doing

- The double-fault restore path (F5) and inter-rename fsync (F6) were DROPped: recovery (`recover_space_staging`) already reconciles every crash combination, and the fixes cost readability/an extra sync for cosmic-ray-frequency events. See `history/assessment-materialize-space-durability.md`.

## Comments

### 2026-08-15T16:26:54Z · @agent

Implemented in fa957a2: generation-dir + current-pointer store for spaces and live overlays (F13 + F14); backward-compatible reads for legacy flat spaces + two-file overlays; deterministic crash-consistency tests; full suite + test-security.sh green.

### 2026-08-15T17:11:32Z · @agent

Review-hardening applied in 1e04534 after /llm-review (gemini-3.1-pro, gpt-5.6-sol, opus-4.7, deepseek-v4-pro) + /assess-findings. All 8 confirmed/introduced findings fixed (conservative recovery on unresolvable pointer, symlink-safe recovery, unconditional parent fsync, no legacy downgrade on corrupt pointer, restored-.old reconciliation, startup live reconcile, keyed page/space collision guard, pointer path-safety). 6 pre-existing/orthogonal items filed as follow-ups: hosted-idem-sweep-robustness, hosted-gc-swap-on-partial-fsync, hosted-genptr-autoheal, hosted-loadbudget-asset-caps, hosted-store-input-revalidation, hosted-snapshot-arc-sharing. Assessment: history/assessment-hosted-store-generation-pointer.md. Full suite + test-security.sh (48 + Wave 2a) green.

