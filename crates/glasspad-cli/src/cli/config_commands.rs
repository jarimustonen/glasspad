use super::publish::{
    publish_config_candidates, resolve_publish_config, resolve_setting, resolve_target,
};
use super::runtime::*;
use super::skill::bundled_skills;
use super::*;

// --- config ---------------------------------------------------------------

/// The four resolution layers exposed by `glasspad config show` (AI-first §8).
/// Kept as strings at the CLI boundary so the JSON contract is explicit and stable.
#[derive(Clone, Copy)]
pub(super) enum ConfigSource {
    Flag,
    Env,
    ConfigFile,
    Default,
}

impl ConfigSource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Env => "env",
            Self::ConfigFile => "config-file",
            Self::Default => "default",
        }
    }
}

/// Return the selected home-config path and whether it exists. This reuses the
/// publish candidate ordering: the first existing path wins; when none exists,
/// the documented primary path is still useful diagnostic output and is never
/// created by this read-only command.
pub(super) fn effective_home_config_path() -> Option<(PathBuf, bool)> {
    let candidates = publish_config_candidates();
    let first = candidates.first()?.clone();
    let selected = candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or(first);
    let exists = selected.exists();
    Some((selected, exists))
}

/// `glasspad config path` — report the selected home-config location without
/// reading, creating, or modifying it. A missing file is normal for loopback-only
/// use, so say that explicitly rather than treating it as an error.
pub fn config_path(json: bool) {
    let Some((path, exists)) = effective_home_config_path() else {
        exit_error(
            json,
            2,
            "config_path_unavailable",
            "cannot determine a glasspad config path because neither $HOME nor a platform config directory is available",
            None,
            None,
        );
    };
    let path = path.display().to_string();
    if json {
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "data": { "path": path, "exists": exists },
            "warnings": [],
        }));
    } else if exists {
        println!("config file: {path}");
    } else {
        println!("no config file exists; expected path: {path}");
    }
}

/// Resolve one diagnostic setting without exposing its value. Empty flags and
/// environment variables follow the existing publish behavior: they are unset,
/// allowing the next layer to win.
pub(super) fn config_source(flag: Option<String>, env: &str, file_is_set: bool) -> ConfigSource {
    let set = |value: Option<String>| value.is_some_and(|v| !v.trim().is_empty());
    if set(flag) {
        ConfigSource::Flag
    } else if set(std::env::var(env).ok()) {
        ConfigSource::Env
    } else if file_is_set {
        ConfigSource::ConfigFile
    } else {
        ConfigSource::Default
    }
}

pub(super) fn config_origin_path(
    cfg: &config::ResolvedConfig,
    origin: Option<config::Origin>,
) -> Option<String> {
    match origin {
        Some(config::Origin::Repo) => cfg.repo_config_path.as_ref(),
        Some(config::Origin::Home) => cfg.home_config_path.as_ref(),
        None => None,
    }
    .map(|path| path.display().to_string())
}

/// Check whether a selected API-key source is usable without reading or emitting
/// secret material. A key file only needs to be a regular file here; `publish`
/// remains responsible for bounded reading and its precise error diagnostics.
pub(super) fn api_key_is_set(source: ConfigSource, cfg: &config::ResolvedConfig) -> bool {
    match source {
        ConfigSource::Flag | ConfigSource::Env => true,
        ConfigSource::Default => false,
        ConfigSource::ConfigFile => match cfg.api_key.as_ref() {
            Some(ApiKeySource::Inline(key)) => !key.trim().is_empty(),
            Some(ApiKeySource::Env(name)) => std::env::var(name)
                .ok()
                .is_some_and(|value| !value.trim().is_empty()),
            Some(ApiKeySource::File(path)) => {
                std::fs::metadata(path).is_ok_and(|meta| meta.is_file())
            }
            None => false,
        },
    }
}

/// `glasspad config show` — expose effective hosted connection settings and their
/// provenance. This deliberately never resolves or serializes API-key material:
/// even a caller-supplied `--api-key` becomes only `<set>` in output.
pub fn config_show(server_flag: Option<String>, api_key_flag: Option<String>, json: bool) {
    let cfg = resolve_publish_config(json);
    let Some((config_path, config_file_exists)) = effective_home_config_path() else {
        exit_error(
            json,
            2,
            "config_path_unavailable",
            "cannot determine a glasspad config path because neither $HOME nor a platform config directory is available",
            None,
            None,
        );
    };

    let server_source = config_source(server_flag.clone(), "GLASSPAD_SERVER", cfg.server.is_some());
    let server = resolve_setting(server_flag, "GLASSPAD_SERVER", cfg.server.clone());
    let api_key_source = config_source(api_key_flag, "GLASSPAD_API_KEY", cfg.api_key.is_some());
    let api_key_set = api_key_is_set(api_key_source, &cfg);
    let target_source = config_source(None, "GLASSPAD_TARGET", cfg.target.is_some());
    let target = resolve_target(None, &cfg, json);
    let mut warnings = Vec::new();
    if cfg.bind_repo_ignored {
        warnings.push("repo-local .glasspad.yaml `bind:` was ignored: LAN serving must be enabled by --bind, $GLASSPAD_BIND, or your home config".to_string());
    }

    let data = json!({
        "config_path": config_path,
        "config_file_exists": config_file_exists,
        "server": {
            "value": server,
            "source": server_source.as_str(),
            "path": config_origin_path(&cfg, cfg.server_origin),
        },
        "api_key": {
            "value": if api_key_set { "<set>" } else { "<unset>" },
            "source": api_key_source.as_str(),
            "secret": true,
            "path": config_origin_path(&cfg, cfg.api_key_origin),
        },
        "target": {
            "value": match target { Target::Loopback => "loopback", Target::Hosted => "hosted" },
            "source": target_source.as_str(),
            "path": config_origin_path(&cfg, cfg.target_origin),
        },
        "template": {
            "value": resolve_setting(None, "GLASSPAD_TEMPLATE", cfg.template.clone()),
            "source": config_source(None, "GLASSPAD_TEMPLATE", cfg.template.is_some()).as_str(),
            "path": config_origin_path(&cfg, cfg.template_origin),
        },
        "space_key": {
            "value": resolve_setting(None, "GLASSPAD_SPACE_KEY", cfg.space_key.clone()),
            "source": config_source(None, "GLASSPAD_SPACE_KEY", cfg.space_key.is_some()).as_str(),
            "path": config_origin_path(&cfg, cfg.space_key_origin),
        },
        "bind": {
            "value": resolve_setting(None, "GLASSPAD_BIND", cfg.bind.clone()),
            "source": config_source(None, "GLASSPAD_BIND", cfg.bind.is_some()).as_str(),
            "path": config_origin_path(&cfg, if cfg.bind.is_some() { Some(config::Origin::Home) } else { None }),
        },
        "favicon": {
            "value": cfg.favicon.clone(),
            "source": if cfg.favicon.is_some() { ConfigSource::ConfigFile.as_str() } else { ConfigSource::Default.as_str() },
            "path": config_origin_path(&cfg, cfg.favicon_origin),
        },
    });
    if json {
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "data": data,
            "warnings": warnings,
        }));
    } else {
        for warning in &warnings {
            eprintln!("warning: {warning}");
        }
        println!(
            "config path: {}{}",
            data["config_path"].as_str().unwrap_or_default(),
            if config_file_exists { "" } else { " (missing)" }
        );
        println!(
            "server: {} ({})",
            server.as_deref().unwrap_or("<unset>"),
            server_source.as_str()
        );
        println!(
            "api_key: {} ({})",
            if api_key_set { "<set>" } else { "<unset>" },
            api_key_source.as_str()
        );
        println!(
            "target: {} ({})",
            data["target"]["value"].as_str().unwrap_or_default(),
            target_source.as_str()
        );
        for key in ["template", "space_key", "bind", "favicon"] {
            println!(
                "{key}: {} ({})",
                data[key]["value"].as_str().unwrap_or("<unset>"),
                data[key]["source"].as_str().unwrap_or_default()
            );
        }
    }
}

// --- doctor (AI-first §18) -------------------------------------------------

#[derive(serde::Serialize)]
pub(super) struct DoctorCheck {
    id: &'static str,
    status: &'static str,
    message: String,
    fix_suggestion: Option<String>,
}

impl DoctorCheck {
    fn ok(id: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            status: "ok",
            message: message.into(),
            fix_suggestion: None,
        }
    }

    fn fail(
        id: &'static str,
        message: impl Into<String>,
        fix_suggestion: impl Into<String>,
    ) -> Self {
        Self {
            id,
            status: "fail",
            message: message.into(),
            fix_suggestion: Some(fix_suggestion.into()),
        }
    }
}

pub(super) fn parse_skill_frontmatter(content: &str) -> Result<serde_yaml::Value, String> {
    // `include_str!` preserves checkout line endings. Accept CRLF so a healthy
    // source build on Windows diagnoses the same bundled skill as an LF build.
    let normalized = content.replace("\r\n", "\n");
    let frontmatter = normalized
        .strip_prefix("---\n")
        .and_then(|body| body.split_once("\n---\n"))
        .map(|(frontmatter, _)| frontmatter)
        .ok_or_else(|| "no readable YAML frontmatter".to_string())?;
    serde_yaml::from_str(frontmatter)
        .map_err(|error| format!("malformed YAML frontmatter: {error}"))
}

/// Validate the metadata in the compiled skill bytes against the catalog used by
/// `skill list` and `version`. This reads no installed skill and writes nothing.
pub(super) fn doctor_skill_check() -> DoctorCheck {
    for skill in bundled_skills() {
        let metadata = match parse_skill_frontmatter(skill.content) {
            Ok(metadata) => metadata,
            Err(error) => {
                return DoctorCheck::fail(
                    "skill.bundle",
                    format!("bundled skill {:?} has {error}", skill.name),
                    "Reinstall glasspad from a verified release.",
                );
            }
        };
        let string_field = |key| metadata.get(key).and_then(serde_yaml::Value::as_str);
        let schema_version = metadata
            .get("schema_version")
            .and_then(serde_yaml::Value::as_u64);
        if string_field("name") != Some(skill.name)
            || string_field("cli_version") != Some(skill.cli_version)
            || schema_version != Some(u64::from(skill.schema_version))
        {
            return DoctorCheck::fail(
                "skill.bundle",
                format!(
                    "bundled skill {:?} metadata does not match the running CLI catalog",
                    skill.name
                ),
                "Reinstall glasspad from a verified release.",
            );
        }
    }
    DoctorCheck::ok(
        "skill.bundle",
        format!(
            "{} bundled skill(s) readable and synchronized with glasspad {}",
            bundled_skills().len(),
            env!("CARGO_PKG_VERSION")
        ),
    )
}

/// Diagnose hosted settings without resolving API-key material. The returned
/// message exposes only `<set>` / `<unset>`, matching `config show`'s secret rule.
pub(super) fn doctor_hosted_check(cfg: &config::ResolvedConfig) -> DoctorCheck {
    let target = match resolve_setting(None, "GLASSPAD_TARGET", None) {
        Some(raw) => match Target::parse(&raw) {
            Ok(target) => target,
            Err(message) => {
                return DoctorCheck::fail(
                    "config.hosted",
                    message,
                    "Set $GLASSPAD_TARGET or config key `target` to `loopback` or `hosted`.",
                );
            }
        },
        None => cfg.target.unwrap_or(Target::Loopback),
    };
    let api_key_source = config_source(None, "GLASSPAD_API_KEY", cfg.api_key.is_some());
    let api_key_set = api_key_is_set(api_key_source, cfg);
    if target == Target::Loopback {
        return DoctorCheck::ok(
            "config.hosted",
            format!(
                "target is loopback; hosted settings are not required (api_key {})",
                if api_key_set { "<set>" } else { "<unset>" }
            ),
        );
    }

    let server = resolve_setting(None, "GLASSPAD_SERVER", cfg.server.clone());
    match (server, api_key_set) {
        (Some(server), true) => DoctorCheck::ok(
            "config.hosted",
            format!("hosted settings ready: server {server}, api_key <set>"),
        ),
        (server, key_set) => {
            let missing = match (server.is_none(), key_set) {
                (true, false) => "config keys `server` and `api_key` are unset",
                (true, true) => "config key `server` is unset",
                (false, false) => {
                    "config key `api_key` is unset or its env/file source is unavailable"
                }
                (false, true) => unreachable!(),
            };
            DoctorCheck::fail(
                "config.hosted",
                format!(
                    "hosted target is not ready: {missing}; api_key {}",
                    if key_set { "<set>" } else { "<unset>" }
                ),
                "Set config keys `server` and `api_key`, or $GLASSPAD_SERVER and $GLASSPAD_API_KEY.",
            )
        }
    }
}

/// `glasspad doctor` runs a small, operationally meaningful self-check. It is
/// strictly read-only: config files and compiled skill bytes are only inspected.
/// Every check runs even after a failure so one invocation gives a complete report.
pub fn doctor(json: bool) {
    let mut checks = Vec::new();
    let config_result = match std::env::current_dir() {
        Ok(cwd) => config::resolve(&cwd, &publish_config_candidates()),
        Err(error) => {
            checks.push(DoctorCheck::fail(
                "config.file",
                format!("cannot determine the current directory: {error}"),
                "Run glasspad from an accessible directory.",
            ));
            Err(config::ConfigError {
                code: "cwd_unavailable",
                message: error.to_string(),
            })
        }
    };

    match config_result {
        Ok(cfg) => {
            let mut loaded = Vec::new();
            if let Some(path) = &cfg.repo_config_path {
                loaded.push(format!("repo config {}", path.display()));
            }
            if let Some(path) = &cfg.home_config_path {
                loaded.push(format!("home config {}", path.display()));
            }
            match (loaded.is_empty(), effective_home_config_path()) {
                (false, _) => checks.push(DoctorCheck::ok(
                    "config.file",
                    format!("configuration parsed from {}", loaded.join(" and ")),
                )),
                (true, Some((path, _))) => checks.push(DoctorCheck::ok(
                    "config.file",
                    format!(
                        "no config file present (expected home config {}); loopback defaults remain available",
                        path.display()
                    ),
                )),
                (true, None) => checks.push(DoctorCheck::ok(
                    "config.file",
                    "no config file or home config path is available; loopback defaults remain available",
                )),
            }
            checks.push(doctor_hosted_check(&cfg));
        }
        Err(error) => {
            if !checks.iter().any(|check| check.id == "config.file") {
                checks.push(DoctorCheck::fail(
                    "config.file",
                    error.message,
                    "Correct the reported config file; inspect its location with `glasspad config path`.",
                ));
            }
            checks.push(DoctorCheck::fail(
                "config.hosted",
                "hosted settings could not be checked because configuration resolution failed",
                "Fix `config.file`, then rerun `glasspad doctor`.",
            ));
        }
    }
    checks.push(doctor_skill_check());

    let ok = checks.iter().filter(|check| check.status == "ok").count();
    let warn = checks.iter().filter(|check| check.status == "warn").count();
    let fail = checks.iter().filter(|check| check.status == "fail").count();
    if json {
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "checks": checks,
            "summary": { "ok": ok, "warn": warn, "fail": fail },
        }));
    } else {
        for check in &checks {
            println!(
                "{} [{}] {}",
                check.status.to_ascii_uppercase(),
                check.id,
                check.message
            );
            if let Some(suggestion) = &check.fix_suggestion {
                println!("  fix: {suggestion}");
            }
        }
        println!("summary: {ok} ok, {warn} warn, {fail} fail");
    }
    if fail > 0 {
        std::process::exit(1);
    }
}
