---
created: 2026-08-15
updated: 2026-08-15
type: improvement
status: open
priority: low
related: ['@hosted-store-generation-pointer']
---

# Hosted store: revalidate caller slug/tenant + per-file symlink checks at the storage boundary

## Description

From the hosted-store-generation-pointer review panel (pre-existing; HTTP layer validates and the store is server-private). Store methods (publish, publish_space, update_space, push_round, page_tenant, write_mapping) join caller-supplied slug/tenant and Space page-slug/asset-key into paths without a store-level `valid_*`/path-normal guard, and read meta/baseline/body FILES via read_capped without an individual symlink check (only dirs are checked). GC also dates a dir from an unvalidated meta. Harden: revalidate slug/tenant with valid_space at the storage boundary; reject symlinked meta/body files (symlink_metadata before read, or O_NOFOLLOW); require full meta validation (schema/slug==dir/tenant) before a destructive GC. Raised by openai + deepseek.
