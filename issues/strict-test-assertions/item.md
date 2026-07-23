---
created: 2026-04-11
updated: 2026-04-11
type: chore
reporter: claude
assignee: jari
status: open
priority: normal
slug: starkly-keen-tongue
---

# Tighten validation test assertions from substring to exact matching

_Source: spec validation tests_

## Description

Most validation tests in `tests/spec_parsing.rs` use loose substring matching:

```rust
assert!(errors.iter().any(|e| e.message.contains("some text")));
```

This pattern:
- Masks accidental over-reporting (extra unexpected errors pass unnoticed)
- Doesn't verify error count
- Doesn't verify section attribution
- Allows duplicate errors to slip through

## Proposed fix

Replace with exact assertions where practical:

```rust
assert_eq!(errors.len(), 1, "unexpected errors: {:?}", errors);
assert_eq!(errors[0].message, "exact expected message");
```

For multi-error tests, assert the full set. This makes the test suite a reliable regression guard for validator behavior changes.
