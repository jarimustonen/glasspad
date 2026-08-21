//! The `glasspad` CLI surface (Wave 3a / Phase 3): `serve`, `create`, `open`.
//!
//! Follows the project's AI-first CLI conventions (`AGENTS-AI-FIRST-CLI.md`):
//! strict input validation with informative, actionable errors; a stable,
//! versioned `--json` envelope on every command; errors as a structured envelope
//! on **stderr** with a meaningful exit code (1 = user error, 2 = system error);
//! and no interactive prompts. Paths are plain positional args — no hidden global
//! state, so the commands compose.
//!
//! The commands are three server entry points, a browser opener, and a standalone
//! data helper:
//! * `serve <dir>` drives Phase 2 live directory serving (scan + watch + SSE).
//! * `create <file>` builds a one-artifact space from a single file and serves it
//!   live (its own single-file watch).
//! * `render <file.md>` renders markdown through a reusable template into an
//!   artifact body and serves it live (0.3.0; see `artifact_host::render`).
//! * `build <space> <out>` statically renders a space to self-contained HTML files
//!   (no server, no bind; reuses the scanner + wrap seam — see `crate::build`).
//! * `open <space>` opens a served space's URL in the browser.
//! * `data <file>` parses a legacy CSV/JSON/mbox file to JSON rows (no server).
//!
//! Fragment-vs-full-document detection is **not** re-implemented here: the content
//! route classifies each artifact at serve time (`artifact_host::wrap`), so a
//! file authored either way — full `<!doctype html>` document or bare fragment —
//! is served correctly whether it arrives via `serve` or `create`.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use crate::artifact_host::guards::HostPolicy;
use crate::artifact_host::space::{self, ScanError};
use crate::artifact_host::{self, ArtifactHost, render, wrap};
use crate::build::{self, LibMode};
use crate::config::{self, ApiKeySource, Target};
use crate::favicon;
use crate::hosted::auth::{KeyFileError, KeyTable};
use crate::hosted::{self, HostedConfig};
use crate::pidfile::{self, PidError};
use crate::server::{self, RenderTemplate};
use crate::submissions::SubmissionStore;

/// The `--json` schema version (AI-first §10). Bump on any breaking change to an
/// envelope: removed/renamed field, changed type/nullability, or changed meaning.
pub const SCHEMA_VERSION: u32 = 1;

/// Supported schema versions by payload family. These constants are the single
/// source used by `version --json` and the corresponding emitters.
pub const SUPPORTED_ENVELOPE_SCHEMAS: &[u32] = &[SCHEMA_VERSION];
pub const SUPPORTED_HELP_SCHEMAS: &[u32] = &[crate::help::SCHEMA_VERSION_HELP];

/// The loopback port used when neither `--port` nor `$GLASSPAD_PORT` is set.
pub const DEFAULT_PORT: u16 = 3000;

/// The environment variable that sets the loopback port (AI-first §8: the env name
/// mirrors the `--port` flag).
pub const PORT_ENV: &str = "GLASSPAD_PORT";

mod author;
mod config_commands;
mod data;
mod host_serve;
mod info;
mod publish;
mod runtime;
mod serve;
mod skill;
mod submission_commands;

#[allow(unused_imports)]
pub use author::{build, create, open, render};
pub use config_commands::{config_path, config_show, doctor};
pub use data::data;
pub use host_serve::host_serve;
pub use info::version;
pub use publish::{publish, push_round};
pub use runtime::{exit_error, help_json, resolve_port, stop};
#[allow(unused_imports)]
pub use serve::{loopback_serve, serve};
pub use skill::{SkillAgent, skill_compat, skill_install, skill_list, skill_print};
pub use submission_commands::{await_submission, submissions};

#[cfg(test)]
mod tests;
