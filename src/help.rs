//! Machine-readable `--help --json` projection (AI-first CLI canon §14).
//!
//! The surface is derived from clap's built command tree. Only examples and the
//! environment mappings, metadata clap does not own for this CLI, are supplied by
//! small declarative registries next to the command definitions in `main.rs`.

use std::ffi::{OsStr, OsString};

use clap::builder::ValueHint;
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;

/// Schema of the help document, aligned with the current family help schema.
pub const SCHEMA_VERSION_HELP: u32 = 3;

#[derive(Clone, Copy)]
pub struct Example {
    pub description: &'static str,
    pub argv: &'static [&'static str],
}

#[derive(Serialize)]
struct ExampleInfo {
    description: &'static str,
    argv: Vec<&'static str>,
}

#[derive(Serialize)]
pub struct HelpData {
    pub schema_version_help: u32,
    #[serde(flatten)]
    command: CommandNode,
}

#[derive(Serialize)]
struct CommandNode {
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    long_about: Option<String>,
    aliases: Vec<String>,
    hidden: bool,
    deprecated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    flags: Vec<FlagInfo>,
    positionals: Vec<PositionalInfo>,
    subcommands: Vec<SubcommandSummary>,
    examples: Vec<ExampleInfo>,
}

#[derive(Serialize)]
struct SubcommandSummary {
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<String>,
    aliases: Vec<String>,
    hidden: bool,
    deprecated: bool,
    has_subcommands: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Serialize)]
struct FlagInfo {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    long: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    short: Option<String>,
    long_aliases: Vec<String>,
    short_aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<String>,
    value_names: Vec<String>,
    takes_value: bool,
    multiple: bool,
    required: bool,
    is_global: bool,
    hidden: bool,
    deprecated: bool,
    defaults: Vec<String>,
    accepted_values: Vec<String>,
    accepts_file_paths: bool,
    conflicts_with: Vec<String>,
    requires: Vec<String>,
    required_unless_present: Vec<String>,
    arity: Arity,
    #[serde(skip_serializing_if = "Option::is_none")]
    env: Option<&'static str>,
}

#[derive(Serialize)]
struct Arity {
    min: usize,
    max: Option<usize>,
    repeated: bool,
    multi_value: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value_delimiter: Option<String>,
    require_equals: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Serialize)]
struct PositionalInfo {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<String>,
    value_names: Vec<String>,
    index: usize,
    required: bool,
    multiple: bool,
    accepted_values: Vec<String>,
    accepts_file_paths: bool,
    defaults: Vec<String>,
    deprecated: bool,
}

const HELP_ID: &str = "__glasspad_structured_help";

/// Cheap token check used before constructing clap's command tree. Exact tokens
/// preserve `--` semantics; glasspad has no short alias for `--json`, so no valid
/// short cluster is excluded.
pub fn looks_like_request(args: &[OsString]) -> bool {
    let before_separator = || {
        args.iter()
            .take_while(|arg| arg.as_os_str() != OsStr::new("--"))
    };
    before_separator().any(|arg| arg == "--json")
        && before_separator().any(|arg| arg == "--help" || arg == "-h")
}

/// Resolve `--help --json` before clap's built-in help action exits.
pub fn resolve_request(root: &Command, args: &[OsString]) -> Option<Vec<String>> {
    if !looks_like_request(args) {
        return None;
    }

    // clap's generated `help <command>` branch executes its display action while
    // parsing, before the synthetic global help flag can be inspected. Resolve
    // that generated branch directly so nodes advertised from the built tree are
    // drillable under the same `--help --json` contract.
    if let Some(path) = generated_help_path(args) {
        return Some(path);
    }

    let mut lenient = root
        .clone()
        .ignore_errors(true)
        .disable_help_flag(true)
        .allow_external_subcommands(true)
        .arg(
            Arg::new(HELP_ID)
                .long("help")
                .short('h')
                .action(ArgAction::SetTrue)
                .global(true),
        );
    allow_external_recursively(&mut lenient);
    let argv = std::iter::once(OsString::from(root.get_name())).chain(args.iter().cloned());
    let matches = lenient.try_get_matches_from_mut(argv).ok()?;
    if !matches.get_flag(HELP_ID) || !matches.get_flag("json") {
        return None;
    }

    // `try_get_matches_from_mut` builds this clone, including clap's generated
    // `help` subcommand. Resolve against that same tree so every advertised node
    // can be drilled into instead of diverging between built and unbuilt trees.
    canonical_path(&lenient, &matches)
}

fn generated_help_path(args: &[OsString]) -> Option<Vec<String>> {
    let mut found_help = false;
    let mut path = Vec::new();
    for arg in args
        .iter()
        .take_while(|arg| arg.as_os_str() != OsStr::new("--"))
    {
        if arg == "--json" || arg == "--help" || arg == "-h" {
            continue;
        }
        let token = arg.to_str()?;
        if !found_help {
            if token != "help" {
                return None;
            }
            found_help = true;
        }
        path.push(token.to_string());
    }
    found_help.then_some(path)
}

fn allow_external_recursively(command: &mut Command) {
    for sub in command.get_subcommands_mut() {
        let owned =
            std::mem::replace(sub, Command::new("placeholder")).allow_external_subcommands(true);
        *sub = owned;
        allow_external_recursively(sub);
    }
}

fn canonical_path(root: &Command, matches: &ArgMatches) -> Option<Vec<String>> {
    let mut command = root;
    let mut node = matches;
    let mut path = Vec::new();
    while let Some((name, submatches)) = node.subcommand() {
        let child = command.find_subcommand(name)?;
        path.push(child.get_name().to_string());
        command = child;
        node = submatches;
    }
    Some(path)
}

pub fn navigate<'a>(root: &'a Command, path: &[String]) -> Option<(&'a Command, String)> {
    let mut command = root;
    let mut names = vec![root.get_name().to_string()];
    for name in path {
        let child = command.find_subcommand(name)?;
        command = child;
        names.push(child.get_name().to_string());
    }
    Some((command, names.join(" ")))
}

pub fn build(
    command: &Command,
    command_path: &str,
    examples: fn(&str) -> Vec<Example>,
    env_for: fn(&str, &str) -> Option<&'static str>,
) -> HelpData {
    let mut flags: Vec<_> = command
        .get_arguments()
        .filter(|arg| !arg.is_positional())
        .map(|arg| flag_info(command, command_path, arg, env_for))
        .collect();
    flags.sort_by(|a, b| a.name.cmp(&b.name));

    let mut positionals: Vec<_> = command.get_positionals().map(positional_info).collect();
    positionals.sort_by_key(|arg| arg.index);

    let mut subcommands: Vec<_> = command
        .get_subcommands()
        .map(|child| SubcommandSummary {
            command: format!("{command_path} {}", child.get_name()),
            about: child.get_about().map(ToString::to_string),
            aliases: child
                .get_visible_aliases()
                .map(ToString::to_string)
                .collect(),
            hidden: child.is_hide_set(),
            deprecated: false,
            has_subcommands: child.get_subcommands().next().is_some(),
        })
        .collect();
    subcommands.sort_by(|a, b| a.command.cmp(&b.command));

    let about = command.get_about().map(ToString::to_string);
    let long_about = command
        .get_long_about()
        .map(ToString::to_string)
        .filter(|long| Some(long) != about.as_ref());
    HelpData {
        schema_version_help: SCHEMA_VERSION_HELP,
        command: CommandNode {
            command: command_path.to_string(),
            about,
            long_about,
            aliases: command
                .get_visible_aliases()
                .map(ToString::to_string)
                .collect(),
            hidden: command.is_hide_set(),
            deprecated: false,
            version: command.get_version().map(ToString::to_string),
            flags,
            positionals,
            subcommands,
            examples: examples(command_path)
                .into_iter()
                .map(|example| ExampleInfo {
                    description: example.description,
                    argv: example.argv.to_vec(),
                })
                .collect(),
        },
    }
}

fn flag_info(
    command: &Command,
    command_path: &str,
    arg: &Arg,
    env_for: fn(&str, &str) -> Option<&'static str>,
) -> FlagInfo {
    let action = arg.get_action();
    let takes_value = matches!(action, ArgAction::Set | ArgAction::Append);
    let mut long_aliases: Vec<_> = arg
        .get_all_aliases()
        .unwrap_or_default()
        .into_iter()
        .map(ToString::to_string)
        .collect();
    long_aliases.sort();
    let mut short_aliases: Vec<_> = arg
        .get_all_short_aliases()
        .unwrap_or_default()
        .into_iter()
        .map(|c| c.to_string())
        .collect();
    short_aliases.sort();
    let range = arg.get_num_args();
    let raw_max = range.map_or(0, |r| r.max_values());
    let max = (raw_max != usize::MAX).then_some(raw_max);
    let (requires, required_unless_present) = requirement_edges(arg);

    FlagInfo {
        name: arg.get_id().as_str().to_string(),
        long: arg.get_long().map(ToString::to_string),
        short: arg.get_short().map(|c| c.to_string()),
        long_aliases,
        short_aliases,
        help: arg.get_help().map(ToString::to_string),
        value_names: if takes_value {
            value_names(arg)
        } else {
            Vec::new()
        },
        takes_value,
        multiple: multiple(arg, action),
        required: arg.is_required_set(),
        is_global: arg.is_global_set(),
        hidden: arg.is_hide_set(),
        deprecated: false,
        defaults: safe_defaults(arg),
        accepted_values: accepted_values(arg),
        accepts_file_paths: accepts_file_paths(arg),
        conflicts_with: conflicts_with(command, arg),
        requires,
        required_unless_present,
        arity: Arity {
            min: range.map_or(0, |r| r.min_values()),
            max,
            repeated: matches!(action, ArgAction::Append | ArgAction::Count),
            multi_value: max.is_none_or(|n| n > 1),
            value_delimiter: arg.get_value_delimiter().map(|c| c.to_string()),
            require_equals: arg.is_require_equals_set(),
        },
        env: env_for(command_path, arg.get_id().as_str()),
    }
}

fn positional_info(arg: &Arg) -> PositionalInfo {
    PositionalInfo {
        name: arg.get_id().as_str().to_string(),
        help: arg.get_help().map(ToString::to_string),
        value_names: value_names(arg),
        index: arg
            .get_index()
            .expect("clap assigns positional indexes after build"),
        required: arg.is_required_set(),
        multiple: multiple(arg, arg.get_action()),
        accepted_values: accepted_values(arg),
        accepts_file_paths: accepts_file_paths(arg),
        defaults: safe_defaults(arg),
        deprecated: false,
    }
}

fn value_names(arg: &Arg) -> Vec<String> {
    arg.get_value_names()
        .unwrap_or_default()
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn safe_defaults(arg: &Arg) -> Vec<String> {
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    if arg.get_id().as_str().contains("api_key") {
        if defaults.is_empty() {
            Vec::new()
        } else {
            vec!["<set>".to_string()]
        }
    } else {
        defaults
    }
}

fn accepted_values(arg: &Arg) -> Vec<String> {
    arg.get_possible_values()
        .into_iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| value.get_name().to_string())
        .collect()
}

fn accepts_file_paths(arg: &Arg) -> bool {
    matches!(
        arg.get_value_hint(),
        ValueHint::AnyPath | ValueHint::FilePath | ValueHint::DirPath | ValueHint::ExecutablePath
    )
}

fn multiple(arg: &Arg, action: &ArgAction) -> bool {
    matches!(action, ArgAction::Append | ArgAction::Count)
        || arg
            .get_num_args()
            .is_some_and(|range| range.max_values() > 1)
}

fn conflicts_with(command: &Command, arg: &Arg) -> Vec<String> {
    if arg.is_global_set() {
        return Vec::new();
    }
    let mut values: Vec<_> = command
        .get_arg_conflicts_with(arg)
        .into_iter()
        .map(|other| other.get_id().as_str().to_string())
        .collect();
    values.sort();
    values.dedup();
    values
}

// clap exposes no requirement getters. Recover unconditional `requires` and
// any-of `required_unless_present` through Arg's Debug projection, guarded by
// focused tests, rather than maintaining a second list. Conditional `requires_if`
// and all-of `required_unless_present_all` are intentionally not represented.
fn requirement_edges(arg: &Arg) -> (Vec<String>, Vec<String>) {
    let debug = format!("{arg:?}");
    let requires = debug_field_list(&debug, "requires").map_or_else(Vec::new, |segment| {
        sorted_dedup(
            segment
                .match_indices("(IsPresent, \"")
                .filter_map(|(index, marker)| {
                    let rest = &segment[index + marker.len()..];
                    rest.find('"').map(|end| rest[..end].to_string())
                })
                .collect(),
        )
    });
    let unless = debug_field_list(&debug, "r_unless")
        .map_or_else(Vec::new, |segment| sorted_dedup(quoted_tokens(segment)));
    (requires, unless)
}

fn debug_field_list<'a>(debug: &'a str, field: &str) -> Option<&'a str> {
    let needle = format!("{field}: [");
    let bytes = debug.as_bytes();
    let mut index = 0;
    let mut quoted = false;
    let mut escaped = false;
    while index < bytes.len() {
        if quoted {
            match bytes[index] {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => quoted = false,
                _ => {}
            }
            index += 1;
        } else if bytes[index] == b'"' {
            quoted = true;
            index += 1;
        } else if debug[index..].starts_with(&needle) {
            return bracket_payload(debug, index + needle.len());
        } else {
            index += 1;
        }
    }
    None
}

fn bracket_payload(debug: &str, start: usize) -> Option<&str> {
    let mut depth = 1;
    for (offset, character) in debug[start..].char_indices() {
        match character {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&debug[start..start + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

fn quoted_tokens(mut segment: &str) -> Vec<String> {
    let mut values = Vec::new();
    while let Some(open) = segment.find('"') {
        let after = &segment[open + 1..];
        let Some(close) = after.find('"') else { break };
        values.push(after[..close].to_string());
        segment = &after[close + 1..];
    }
    values
}

fn sorted_dedup(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_projection_is_pinned_to_clap_debug_shape() {
        let required = Arg::new("a").long("a").requires("b").requires("c");
        assert_eq!(requirement_edges(&required).0, ["b", "c"]);

        let unless = Arg::new("u").long("u").required_unless_present("v");
        assert_eq!(requirement_edges(&unless).1, ["v"]);
        assert_eq!(
            requirement_edges(&Arg::new("z").long("z")),
            (Vec::new(), Vec::new())
        );
    }

    #[test]
    fn api_key_defaults_are_never_serialized() {
        let arg = Arg::new("api_key")
            .long("api-key")
            .default_value("SECRET_SENTINEL");
        assert_eq!(safe_defaults(&arg), ["<set>"]);
    }
}
