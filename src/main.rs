// The DSL/spec, data parsers, and security primitives live in the library crate
// (`glasspad::{spec,data,security}`) — kept intact for Wave 5. The binary reaches
// them through the lib rather than re-compiling them: with the legacy pad path
// removed (Wave 3, design.md §10 / D2), only `cli` (data parsers) and
// `artifact_host` (security::token) still consume them, so re-`mod`-ing the full
// DSL into the binary would only manufacture dead-code warnings.
mod artifact_host;
mod cli;
mod docs;
mod server;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "glasspad", about = "AI scratchpad for rich data views")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the glasspad server
    Serve {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        /// Directory to serve live as a space (Wave 2a). Omit to serve only the
        /// built-in fixtures; Wave 3a formalizes the full `serve ./dir` contract.
        dir: Option<PathBuf>,
    },
    /// Create a new pad from a file or stdin
    Create {
        #[arg(short, long)]
        file: Option<PathBuf>,
        /// Attach a dataset: --data events=events.csv
        #[arg(long = "data", value_parser = cli::parse_data_arg)]
        data: Vec<(String, PathBuf)>,
    },
    /// List all pads
    List,
    /// Open a pad in the browser
    Open {
        /// Pad ID
        id: String,
    },
    /// Show documentation (spec, sections, charts, examples, api)
    Docs {
        /// Topic: spec, sections, charts, examples, api
        topic: Option<String>,
    },
    /// Output or install the Claude Code skill
    Skill {
        /// Install to .claude/skills/. Project-level by default, --user for ~/.claude/
        #[arg(long)]
        install_claude: bool,
        /// Use with --install-claude: install to ~/.claude/ instead of project
        #[arg(long, requires = "install_claude")]
        user: bool,
    },
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    match args.command {
        Commands::Serve { port, dir } => server::run_dir(port, dir).await,
        Commands::Create { file, data } => cli::create(file, data, None).await,
        Commands::List => cli::list(None).await,
        Commands::Open { id } => cli::open(id, None).await,
        Commands::Docs { topic } => match topic.as_deref() {
            None => docs::print_index(),
            Some("spec") => docs::print_spec(),
            Some("sections") => docs::print_sections(),
            Some("charts") => docs::print_charts(),
            Some("examples") => docs::print_examples(),
            Some("api") => docs::print_api(),
            Some(other) => {
                eprintln!("Unknown topic: {}", other);
                eprintln!("Available: spec, sections, charts, examples, api");
                std::process::exit(1);
            }
        },
        Commands::Skill { install_claude, user } => cli::skill(install_claude, user),
    }
}
