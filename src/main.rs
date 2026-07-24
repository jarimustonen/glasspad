// The DSL/spec, data parsers, and security primitives live in the library crate
// (`glasspad::{spec,data,security}`) — kept intact for Wave 5. The binary reaches
// them through the lib rather than re-compiling them: with the legacy pad path
// removed (Wave 3, design.md §10 / D2), only `artifact_host` (security::token)
// still consumes them from the binary side, so re-`mod`-ing the full DSL into the
// binary would only manufacture dead-code warnings. (The `data`/`spec` parsers
// stay exported from the lib for Wave 5's optional `glasspad data` helper.)
mod artifact_host;
mod cli;
mod server;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "glasspad", about = "AI scratchpad for rich HTML artifact views")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Emit machine-readable JSON (stable, versioned envelopes; errors to stderr).
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Serve a directory live as a space (scan + watch + SSE). With no directory,
    /// serves the built-in fixtures. Runs until killed.
    Serve {
        /// Directory to serve as a space. Omit to serve only the built-in fixtures.
        dir: Option<PathBuf>,
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
    /// Build a one-artifact space from a single file and serve it live.
    Create {
        /// The HTML file to serve (fragment or full document — auto-detected).
        file: PathBuf,
        /// Space name (default: the file stem). Must match the space grammar.
        #[arg(long)]
        name: Option<String>,
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
    /// Open a served space's URL in the browser.
    Open {
        /// Space name (the `{space}` in `/{space}/`).
        space: String,
        #[arg(short, long, default_value = "3000")]
        port: u16,
        /// Print the URL without launching a browser (pipe-friendly).
        #[arg(long)]
        no_browser: bool,
    },
    /// Output or install the Claude Code skill (the CLI's operating manual).
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
    let json = args.json;

    match args.command {
        Commands::Serve { dir, port } => cli::serve(dir, port, json).await,
        Commands::Create { file, name, port } => cli::create(file, name, port, json).await,
        Commands::Open { space, port, no_browser } => cli::open(space, port, json, no_browser),
        Commands::Skill { install_claude, user } => cli::skill(install_claude, user),
    }
}
