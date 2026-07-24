//! The `glasspad` library crate.
//!
//! With the legacy section-DSL content path removed (Wave 5 / Phase 6), the lib
//! exposes two things the HTML-artifact host still needs:
//! * [`security`] — the [`security::token`] nonce generator the artifact wrapper
//!   uses when embedding per-response markers.
//! * [`data`] — the tabular parsers (CSV / JSON / mbox) that back the optional
//!   `glasspad data` CLI helper, which parses those legacy formats on demand.

pub mod data;
pub mod security;
