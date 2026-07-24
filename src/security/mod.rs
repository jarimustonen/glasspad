//! Security primitives for the HTML-artifact host.
//!
//! Only [`token`] survives Wave 5: it generates the unguessable nonces the
//! artifact wrapper embeds. The old section-DSL sanitiser (`ammonia`-based HTML
//! scrubbing) and the JSON-script embedder existed only to safely inline
//! server-parsed data into the legacy dashboard renderer, which is gone — the
//! new model sandboxes every artifact in a null-origin iframe instead, so those
//! mechanisms were removed rather than demoted.

pub mod token;
