use super::runtime::*;
use super::*;

// --- skill (AI-first §15/§16) ---------------------------------------------

/// One skill compiled into the binary. Content, versions, and source path live
/// together as the shared inventory for `version`, `list`, `print`, and `install`.
pub(super) struct BundledSkill {
    pub(super) name: &'static str,
    description: &'static str,
    pub(super) cli_version: &'static str,
    pub(super) schema_version: u32,
    pub(super) content: &'static str,
    path_in_repo: &'static str,
}

pub(super) const BUNDLED_SKILLS: &[BundledSkill] = &[BundledSkill {
    name: "glasspad",
    description: "Show rich visual HTML views (dashboards, charts, interactive UIs) to the user in their browser. Use when asked to visualize, plot, chart, dashboard, or \"show me\" something.",
    cli_version: env!("CARGO_PKG_VERSION"),
    schema_version: 1,
    content: include_str!("../skill.md"),
    path_in_repo: "crates/glasspad-cli/src/skill.md",
}];

pub(super) const DEFAULT_SKILL: &str = BUNDLED_SKILLS[0].name;

pub(super) fn bundled_skills() -> &'static [BundledSkill] {
    BUNDLED_SKILLS
}

pub(super) fn skill_metadata(skill: &BundledSkill) -> serde_json::Value {
    json!({
        "name": skill.name,
        "cli_version": skill.cli_version,
        "schema_version": skill.schema_version,
    })
}

pub(super) fn bundled_skill_metadata() -> Vec<serde_json::Value> {
    bundled_skills().iter().map(skill_metadata).collect()
}

pub(super) fn find_bundled_skill(name: &str, json: bool) -> &'static BundledSkill {
    if let Some(skill) = bundled_skills().iter().find(|skill| skill.name == name) {
        return skill;
    }
    let available: Vec<String> = bundled_skills()
        .iter()
        .map(|skill| skill.name.to_string())
        .collect();
    exit_error(
        json,
        1,
        "skill_not_found",
        &format!(
            "no bundled skill named {name:?}; available: {}",
            available.join(", ")
        ),
        Some(name),
        Some(available),
    );
}

/// `glasspad skill list` lists the skills compiled into this binary. The JSON
/// envelope exposes the exact metadata also returned by `version --json`.
pub fn skill_list(json: bool) {
    if json {
        let skills: Vec<_> = bundled_skills()
            .iter()
            .map(|skill| {
                let mut metadata = skill_metadata(skill);
                metadata["description"] = json!(skill.description);
                metadata
            })
            .collect();
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "data": { "skills": skills },
            "warnings": [],
        }));
    } else {
        for skill in bundled_skills() {
            println!(
                "{}\tcli {}\tschema {}\t{}",
                skill.name, skill.cli_version, skill.schema_version, skill.description
            );
        }
    }
}

/// `glasspad skill print <name>` streams the canonical bundled bytes without any
/// filesystem or network access. JSON mode separates metadata from content.
pub fn skill_print(name: &str, json: bool) {
    let skill = find_bundled_skill(name, json);
    if json {
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "name": skill.name,
            "cli_version": skill.cli_version,
            "schema_version_skill": skill.schema_version,
            "content": skill.content,
            "path_in_repo": skill.path_in_repo,
        }));
    } else {
        print!("{}", skill.content);
    }
}

/// Which agent skill directory(ies) `skill install` writes into.
///
/// The CLI ships one companion skill (`SKILL.md`); the migration from Claude Code
/// to pi.dev means the *same* skill must be discoverable under both harnesses.
/// Claude Code loads `~/.claude/skills/<name>/SKILL.md`; pi.dev loads
/// `~/.pi/agent/skills/<name>/SKILL.md` (and invokes it as `/skill:name`). Rather
/// than force the caller to run the installer twice, `--agent all` (the default)
/// *dual-homes* — one invocation writes both. This mirrors the agent-target
/// convention already used by deployment automation (`--agent claude|…|all`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum SkillAgent {
    /// Claude Code skills dir (`~/.claude/skills/` for `--user`, else `./.claude/skills/`).
    Claude,
    /// pi.dev skills dir (`~/.pi/agent/skills/` for `--user`, else `./.pi/skills/`).
    Pi,
    /// Dual-home into both Claude Code and pi.dev (the default).
    All,
}

/// Write `content` to `<dir>/SKILL.md`, creating the tree as needed, and report
/// whether the file was freshly created (vs. overwritten). Any I/O failure exits
/// via the structured-error contract (exit 2), so this never returns on error.
///
/// `created` is decided atomically with an exclusive `create_new` open: it tells a
/// fresh install apart from an in-place refresh even under a racing installer,
/// where a plain `exists()`-then-`write` would misreport. The returned path is
/// canonicalized (the file now exists) for a stable absolute path in the envelope.
///
/// The refresh path refuses to follow a symlinked `SKILL.md`: a planted
/// `…/SKILL.md -> /some/sensitive/file` would otherwise be truncated with the skill
/// content on overwrite. This matches the scanner's `symlink_rejected` policy — the
/// installer never writes *through* a symlink at the destination.
pub(super) fn write_skill_file(dir: &Path, content: &str, json: bool) -> (PathBuf, bool) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        exit_error(
            json,
            2,
            "io_error",
            &format!("cannot create {}: {e}", dir.display()),
            None,
            None,
        );
    }
    let path = dir.join("SKILL.md");
    use std::io::Write as _;
    let created = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut f) => {
            if let Err(e) = f.write_all(content.as_bytes()) {
                exit_error(
                    json,
                    2,
                    "io_error",
                    &format!("cannot write {}: {e}", path.display()),
                    None,
                    None,
                );
            }
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // The name already exists — refuse to overwrite *through* a symlink
            // (CWE-59). `symlink_metadata` does not follow the link, so a symlinked
            // destination is rejected with a stable code rather than truncating its
            // target. A real file (or hard link) falls through to the refresh write.
            if std::fs::symlink_metadata(&path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                exit_error(
                    json,
                    1,
                    "symlink_rejected",
                    &format!(
                        "refusing to install over {}: destination is a symlink",
                        path.display()
                    ),
                    None,
                    None,
                );
            }
            if let Err(e) = std::fs::write(&path, content) {
                exit_error(
                    json,
                    2,
                    "io_error",
                    &format!("cannot write {}: {e}", path.display()),
                    None,
                    None,
                );
            }
            false
        }
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot write {}: {e}", path.display()),
            None,
            None,
        ),
    };
    // Prefer the canonical absolute path (the file now exists, so this resolves),
    // falling back to the join if canonicalization fails.
    let resolved = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    (resolved, created)
}

/// `glasspad skill install [<name>]` installs one bundled skill, or every bundled
/// skill when the name is omitted. `--user` selects home scope and
/// `--agent {claude|pi|all}` selects the runtime directories.
///
/// Under `--json`, an install emits the AI-first §10 success envelope
/// (`{schema_version, installed, scope, path, created, targets, cli_version,
/// warnings}`); the top-level `path`/`scope`/`created` describe the first target
/// (Claude when selected) for backward compatibility, while `targets[]` reports
/// every path written. The error path (missing `.claude/`, unwritable HOME, a
/// symlinked destination) uses the shared [`exit_error`] contract (structured
/// error on stderr, non-zero exit).
///
/// Partial-failure semantics: skills use inventory order, with Claude then pi for
/// each skill. If a later write fails, earlier targets remain in place. The install
/// is idempotent, so re-running completes or refreshes every selected target.
pub fn skill_install(name: Option<&str>, user: bool, agent: Option<SkillAgent>, json: bool) {
    let selected: Vec<&BundledSkill> = match name {
        Some(name) => vec![find_bundled_skill(name, json)],
        None => bundled_skills().iter().collect(),
    };

    // `--agent` defaults to dual-home when omitted. Resolving it here (rather than
    // via a clap `default_value_t`) keeps the flag genuinely optional so its
    // `requires = "install_claude"` fires only on an explicit `--agent`.
    let agent = agent.unwrap_or(SkillAgent::All);
    let scope = if user { "user" } else { "project" };

    // Resolve HOME once because both agent targets share it under `--user`.
    let home = if user {
        match dirs::home_dir() {
            Some(h) => Some(h),
            None => exit_error(
                json,
                2,
                "home_dir_not_found",
                "cannot determine home directory for a --user install ($HOME unset)",
                None,
                None,
            ),
        }
    } else {
        None
    };

    let want_claude = matches!(agent, SkillAgent::Claude | SkillAgent::All);
    let want_pi = matches!(agent, SkillAgent::Pi | SkillAgent::All);
    let claude_base = if want_claude {
        Some(match &home {
            Some(h) => h.join(".claude"),
            None => {
                let claude_dir = PathBuf::from(".claude");
                if !claude_dir.exists() {
                    exit_error(
                        json,
                        1,
                        "claude_dir_not_found",
                        ".claude/ directory not found in current directory. \
                         Are you in a project root? Use --install-claude --user for a user-level install, \
                         or --agent pi to install only the pi.dev skill (which needs no .claude/).",
                        None,
                        None,
                    );
                }
                claude_dir
            }
        })
    } else {
        None
    };
    let pi_base = if want_pi {
        Some(match &home {
            Some(h) => h.join(".pi/agent"),
            None => PathBuf::from(".pi"),
        })
    } else {
        None
    };

    // Inventory order is stable. For each skill, Claude precedes pi, preserving
    // the legacy first-target fields for today's default skill.
    let mut targets: Vec<(&str, &str, PathBuf, bool)> = Vec::new();
    for skill in selected {
        if let Some(base) = &claude_base {
            let (path, created) =
                write_skill_file(&base.join("skills").join(skill.name), skill.content, json);
            targets.push((skill.name, "claude", path, created));
        }
        if let Some(base) = &pi_base {
            let (path, created) =
                write_skill_file(&base.join("skills").join(skill.name), skill.content, json);
            targets.push((skill.name, "pi", path, created));
        }
    }

    if json {
        let targets_json: Vec<_> = targets
            .iter()
            .map(|(skill, agent, path, created)| {
                json!({
                    "skill": skill,
                    "agent": agent,
                    "scope": scope,
                    "path": path.display().to_string(),
                    "created": created,
                })
            })
            .collect();
        let (_, _, first_path, first_created) = match targets.first() {
            Some(t) => t,
            None => exit_error(
                json,
                2,
                "no_agent_selected",
                "no skill install target was selected (internal invariant violated)",
                None,
                None,
            ),
        };
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "installed": true,
            "scope": scope,
            "path": first_path.display().to_string(),
            "created": first_created,
            "targets": targets_json,
            "cli_version": env!("CARGO_PKG_VERSION"),
            "warnings": [],
        }));
    } else {
        for (_, _, path, _) in &targets {
            println!("Installed skill to {}", path.display());
        }
    }
}

/// Preserve the pre-subcommand surface. Bare `glasspad skill` remains the original
/// side-effect-free content dump, while `--install` / `--install-claude` route to
/// the canonical installer implementation.
pub fn skill_compat(install: bool, user: bool, agent: Option<SkillAgent>, json: bool) {
    if install {
        // The flattened compatibility surface predates install-all semantics and
        // continues to install only glasspad if more skills are bundled later.
        skill_install(Some(DEFAULT_SKILL), user, agent, json);
    } else {
        // Legacy `skill --json` also emitted raw markdown. Preserve those bytes;
        // canonical structured inspection is `skill print glasspad --json`.
        skill_print(DEFAULT_SKILL, false);
    }
}
