//! Publish-target configuration (the `publish`-first surface).
//!
//! `publish` is the default verb; *where* it lands is resolved from config, not a
//! flag. A `target` (`loopback` | `hosted`) plus the settings each target needs
//! (`server`, `api_key`, default `template`, default `space_key`, `favicon`) are
//! merged **per key** across two files, first file that sets a given key wins:
//!
//! 1. **`.glasspad.yaml`** — the repo-local config, found by walking up from the
//!    current directory (like `.git`). This is a NEW file, distinct from the
//!    per-space `glasspad.yaml` (which stays structure-only: nav/title/theme).
//! 2. **`~/.config/glasspad/config.yaml`** — the home config (honoring
//!    `$XDG_CONFIG_HOME` and the legacy `dirs::config_dir()` fallback).
//! 3. Built-in default — `target: loopback` (so with *no* config at all, `publish`
//!    still serves loopback: zero-config local just works).
//!
//! Because the merge is per-key, a repo can set only `target`/`favicon` and inherit
//! `server`+`api_key` from the home config.
//!
//! The `api_key` accepts an **indirection** (an env var or a key file), not only an
//! inline plaintext secret, so a later multi-worker credential model
//! (`hosted-multiworker-credentials`) can layer on without a schema break. The
//! source is captured here and only resolved to the actual secret at use time.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Where `publish` lands. Resolved from config (per-key), overridable by a
/// `--target` flag / `$GLASSPAD_TARGET`, defaulting to [`Target::Loopback`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Serve on `127.0.0.1` with live reload (the default; folds serve/create/render/open).
    Loopback,
    /// Upload the space to a hosted share server, returning a `/p/<slug>/…` URL.
    Hosted,
}

impl Target {
    /// Parse a `target:` value (case-insensitive). Strict — an unknown value is a
    /// user error, never a silent fallback.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "loopback" => Ok(Target::Loopback),
            "hosted" => Ok(Target::Hosted),
            other => Err(format!(
                "invalid target {other:?}: expected `loopback` or `hosted`"
            )),
        }
    }
}

/// Where the hosted `api_key` comes from. An inline secret still works; the
/// indirection forms keep the plaintext out of the config file and let a future
/// credential model source a scoped/rotatable token without a schema break.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiKeySource {
    /// The literal key, inline in the config (fine for a single operator).
    Inline(String),
    /// Read the key from this environment variable at publish time.
    Env(String),
    /// Read the key from this file at publish time (trimmed of trailing newline).
    File(PathBuf),
}

/// The fully-resolved publish config after the per-key merge. Every field is
/// optional here; a flag / env override still takes precedence above it, and the
/// built-in `target: loopback` default is applied by the caller when `target` is
/// `None`.
#[derive(Clone, Debug, Default)]
pub struct ResolvedConfig {
    pub target: Option<Target>,
    pub server: Option<String>,
    pub api_key: Option<ApiKeySource>,
    pub template: Option<String>,
    pub space_key: Option<String>,
    /// Emoji favicon — reserved for the `emoji-favicon` feature. Parsed and carried
    /// here (so a config that sets it is accepted, per the design's per-key schema)
    /// but not yet consumed by a publish path; `allow(dead_code)` until it lands.
    #[allow(dead_code)]
    pub favicon: Option<String>,
}

/// A structured config error carrying a stable `code` + human `message` so the CLI
/// can surface it through its `--json` error envelope without this module depending
/// on the CLI's exit machinery.
#[derive(Debug)]
pub struct ConfigError {
    pub code: &'static str,
    pub message: String,
}

impl ConfigError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// The on-disk schema for both `.glasspad.yaml` and the home config. All fields
/// optional; unknown keys are ignored (forward-compatible). `api_key` accepts
/// either an inline string or an `{env: …}` / `{file: …}` mapping; `api_key_file`
/// is a convenience spelling of `api_key: {file: …}`.
#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    target: Option<String>,
    server: Option<String>,
    api_key: Option<ApiKeyField>,
    api_key_file: Option<String>,
    template: Option<String>,
    space_key: Option<String>,
    favicon: Option<String>,
}

/// `api_key:` in YAML — either a bare string (inline) or an indirection mapping.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ApiKeyField {
    Inline(String),
    Indirect(ApiKeyIndirect),
}

#[derive(Debug, Deserialize)]
struct ApiKeyIndirect {
    env: Option<String>,
    file: Option<String>,
}

impl ConfigFile {
    /// The `api_key` source this file declares, if any: `api_key` (inline or
    /// indirect) wins over the `api_key_file` convenience key. Returns `None` when
    /// the file sets neither, `Err` when the mapping is malformed (empty, or both
    /// `env` and `file`).
    fn api_key_source(&self, origin: &Path) -> Result<Option<ApiKeySource>, ConfigError> {
        if let Some(field) = &self.api_key {
            let src = match field {
                ApiKeyField::Inline(s) => ApiKeySource::Inline(s.clone()),
                ApiKeyField::Indirect(i) => match (&i.env, &i.file) {
                    (Some(e), None) => ApiKeySource::Env(e.clone()),
                    (None, Some(f)) => ApiKeySource::File(PathBuf::from(f)),
                    (Some(_), Some(_)) => {
                        return Err(ConfigError::new(
                            "invalid_config",
                            format!(
                                "malformed {}: api_key sets both `env` and `file`; set exactly one",
                                origin.display()
                            ),
                        ));
                    }
                    (None, None) => {
                        return Err(ConfigError::new(
                            "invalid_config",
                            format!(
                                "malformed {}: api_key mapping must set `env` or `file`",
                                origin.display()
                            ),
                        ));
                    }
                },
            };
            return Ok(Some(src));
        }
        if let Some(f) = &self.api_key_file {
            return Ok(Some(ApiKeySource::File(PathBuf::from(f))));
        }
        Ok(None)
    }
}

/// One loaded config file plus the path it came from (for error messages).
struct Loaded {
    file: ConfigFile,
    path: PathBuf,
}

/// Resolve the publish config by the per-key merge described in the module docs.
/// Reads the repo-local `.glasspad.yaml` (walking up from `cwd`) and the home
/// config; a file that is absent is skipped, one that exists-but-is-unreadable or
/// malformed is a hard, informative error (never silently ignored — that could
/// substitute a different server/key). The `home_candidates` are tried in order;
/// the first that exists is the home config.
pub fn resolve(cwd: &Path, home_candidates: &[PathBuf]) -> Result<ResolvedConfig, ConfigError> {
    let repo = match find_repo_config(cwd) {
        Some(path) => load_file(&path)?,
        None => None,
    };
    let home = load_first_existing(home_candidates)?;
    merge(repo, home)
}

/// Merge the (optional) repo and home files per key: for each key, the repo value
/// wins if set, else the home value, else unset.
fn merge(repo: Option<Loaded>, home: Option<Loaded>) -> Result<ResolvedConfig, ConfigError> {
    let repo_file = repo.as_ref().map(|l| &l.file);
    let home_file = home.as_ref().map(|l| &l.file);

    // `target` is validated the moment it is chosen so a bad value in the *winning*
    // file is reported (a bad value in an overridden file is irrelevant).
    let target_raw = repo_file
        .and_then(|f| f.target.clone())
        .or_else(|| home_file.and_then(|f| f.target.clone()));
    let target = match target_raw {
        Some(raw) => Some(Target::parse(&raw).map_err(|m| ConfigError::new("invalid_target", m))?),
        None => None,
    };

    let server = repo_file
        .and_then(|f| f.server.clone())
        .or_else(|| home_file.and_then(|f| f.server.clone()));
    let template = repo_file
        .and_then(|f| f.template.clone())
        .or_else(|| home_file.and_then(|f| f.template.clone()));
    let space_key = repo_file
        .and_then(|f| f.space_key.clone())
        .or_else(|| home_file.and_then(|f| f.space_key.clone()));
    let favicon = repo_file
        .and_then(|f| f.favicon.clone())
        .or_else(|| home_file.and_then(|f| f.favicon.clone()));

    // `api_key` resolves per-file (each file's `api_key`/`api_key_file` is a unit),
    // repo winning over home.
    let repo_key = match &repo {
        Some(l) => l.file.api_key_source(&l.path)?,
        None => None,
    };
    let api_key = match repo_key {
        Some(k) => Some(k),
        None => match &home {
            Some(l) => l.file.api_key_source(&l.path)?,
            None => None,
        },
    };

    Ok(ResolvedConfig {
        target,
        server: nonempty(server),
        api_key,
        template: nonempty(template),
        space_key: nonempty(space_key),
        favicon: nonempty(favicon),
    })
}

/// Trim and drop empty/whitespace-only strings to `None` (AI-first §1: an empty
/// config value is treated as unset, never as a silent empty).
fn nonempty(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Walk up from `start` to the filesystem root looking for a `.glasspad.yaml`,
/// returning the first one found (the repo-local config). Bounded so a symlink loop
/// or pathological depth can't spin forever.
fn find_repo_config(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    for _ in 0..64 {
        let cur = dir?;
        let candidate = cur.join(".glasspad.yaml");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = cur.parent();
    }
    None
}

/// Load a single config file. `NotFound` → `Ok(None)` (skip). An existing but
/// unreadable file, or a malformed one, is a hard error.
fn load_file(path: &Path) -> Result<Option<Loaded>, ConfigError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(ConfigError::new(
                "unreadable_config",
                format!("cannot read {}: {e}", path.display()),
            ));
        }
    };
    let file: ConfigFile = serde_yaml::from_str(&contents).map_err(|e| {
        ConfigError::new(
            "invalid_config",
            format!("malformed {}: {e}", path.display()),
        )
    })?;
    Ok(Some(Loaded {
        file,
        path: path.to_path_buf(),
    }))
}

/// Return the first candidate that exists, loaded. An existing-but-broken candidate
/// is a hard error (it must not be silently skipped to a different candidate, which
/// could substitute a different server/key).
fn load_first_existing(candidates: &[PathBuf]) -> Result<Option<Loaded>, ConfigError> {
    for path in candidates {
        if let Some(loaded) = load_file(path)? {
            return Ok(Some(loaded));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        p
    }

    fn tmp() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = std::env::temp_dir().join(format!("gp-cfg-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn target_parse_is_strict() {
        assert_eq!(Target::parse("loopback"), Ok(Target::Loopback));
        assert_eq!(Target::parse(" HOSTED "), Ok(Target::Hosted));
        assert!(Target::parse("remote").is_err());
    }

    #[test]
    fn no_config_resolves_to_all_none() {
        let dir = tmp();
        let cfg = resolve(&dir, &[]).unwrap();
        assert_eq!(cfg.target, None);
        assert_eq!(cfg.server, None);
        assert!(cfg.api_key.is_none());
    }

    #[test]
    fn per_key_merge_repo_wins_but_inherits_home() {
        let dir = tmp();
        // Repo sets target + favicon only; home carries server + inline api_key.
        write(&dir, ".glasspad.yaml", "target: hosted\nfavicon: \"🌟\"\n");
        let home = write(
            &dir,
            "home/config.yaml",
            "server: https://h.example\napi_key: sekret\ntarget: loopback\n",
        );

        let cfg = resolve(&dir, &[home]).unwrap();
        // target: repo wins over home.
        assert_eq!(cfg.target, Some(Target::Hosted));
        assert_eq!(cfg.favicon.as_deref(), Some("🌟"));
        // server + api_key inherited from home.
        assert_eq!(cfg.server.as_deref(), Some("https://h.example"));
        assert_eq!(cfg.api_key, Some(ApiKeySource::Inline("sekret".into())));
    }

    #[test]
    fn api_key_env_indirection() {
        let dir = tmp();
        let home = write(&dir, "config.yaml", "api_key:\n  env: GLASSPAD_API_KEY\n");
        let cfg = resolve(&dir, &[home]).unwrap();
        assert_eq!(
            cfg.api_key,
            Some(ApiKeySource::Env("GLASSPAD_API_KEY".into()))
        );
    }

    #[test]
    fn api_key_file_indirection_and_convenience_key() {
        let dir = tmp();
        let home = write(
            &dir,
            "map/config.yaml",
            "api_key:\n  file: /run/secrets/gp\n",
        );
        let cfg = resolve(&dir, std::slice::from_ref(&home)).unwrap();
        assert_eq!(
            cfg.api_key,
            Some(ApiKeySource::File("/run/secrets/gp".into()))
        );

        let dir2 = tmp();
        let home2 = write(
            &dir2,
            "conv/config.yaml",
            "api_key_file: /run/secrets/gp2\n",
        );
        let cfg2 = resolve(&dir2, std::slice::from_ref(&home2)).unwrap();
        assert_eq!(
            cfg2.api_key,
            Some(ApiKeySource::File("/run/secrets/gp2".into()))
        );
    }

    #[test]
    fn api_key_mapping_with_both_env_and_file_is_rejected() {
        let dir = tmp();
        let home = write(&dir, "config.yaml", "api_key:\n  env: X\n  file: /y\n");
        let err = resolve(&dir, &[home]).unwrap_err();
        assert_eq!(err.code, "invalid_config");
    }

    #[test]
    fn bad_target_is_rejected() {
        let dir = tmp();
        write(&dir, ".glasspad.yaml", "target: nowhere\n");
        let err = resolve(&dir, &[]).unwrap_err();
        assert_eq!(err.code, "invalid_target");
    }

    #[test]
    fn repo_config_found_by_walking_up() {
        let dir = tmp();
        write(&dir, ".glasspad.yaml", "target: hosted\n");
        let nested = dir.join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        let cfg = resolve(&nested, &[]).unwrap();
        assert_eq!(cfg.target, Some(Target::Hosted));
    }

    #[test]
    fn malformed_config_is_a_hard_error() {
        let dir = tmp();
        write(&dir, ".glasspad.yaml", "target: hosted\n  : : bad\n");
        let err = resolve(&dir, &[]).unwrap_err();
        assert_eq!(err.code, "invalid_config");
    }
}
