//! Pure domain logic for `glasspad`.
//!
//! This library crate deliberately contains no clap surface and performs no
//! filesystem, network, process, environment, or wall-clock I/O. The binary and
//! all side-effecting adapters live in `crates/glasspad-cli`.

pub mod data;
pub mod security;
pub mod time;
