// The tabular data parsers and security primitives live in the library crate
// (`glasspad::{data,security}`). The binary reaches them through the lib: the
// artifact host uses `security::token` for per-response nonces, and the optional
// `glasspad data` subcommand uses `glasspad::data` to parse the legacy CSV / JSON
// / mbox formats on demand. The section-DSL renderer that once consumed them was
// removed in Wave 5 / Phase 6.
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
        /// TCP port on 127.0.0.1 (1-65535).
        #[arg(short, long, default_value_t = 3000, value_parser = clap::value_parser!(u16).range(1..))]
        port: u16,
    },
    /// Build a one-artifact space from a single file and serve it live.
    Create {
        /// The HTML file to serve (fragment or full document — auto-detected).
        file: PathBuf,
        /// Space name (default: the file stem). Must match the space grammar.
        #[arg(long)]
        name: Option<String>,
        /// TCP port on 127.0.0.1 (1-65535).
        #[arg(short, long, default_value_t = 3000, value_parser = clap::value_parser!(u16).range(1..))]
        port: u16,
    },
    /// Open a served space's URL in the browser.
    Open {
        /// Space name (the `{space}` in `/{space}/`).
        space: String,
        /// TCP port on 127.0.0.1 (1-65535).
        #[arg(short, long, default_value_t = 3000, value_parser = clap::value_parser!(u16).range(1..))]
        port: u16,
        /// Print the URL without launching a browser (pipe-friendly).
        #[arg(long)]
        no_browser: bool,
    },
    /// Parse a legacy tabular file (CSV / JSON / mbox) to JSON rows on stdout.
    ///
    /// A standalone helper for the old data formats: it parses the file with the
    /// same limits the pre-rewrite server used and prints the rows as JSON (the
    /// data channel), so a caller can reuse those inputs when authoring an
    /// HTML artifact. It never starts a server.
    Data {
        /// The data file to parse. Format is inferred from the extension
        /// (.csv / .json / .mbox|.eml) unless `--format` overrides it.
        file: PathBuf,
        /// Force the input format instead of inferring from the extension.
        #[arg(long, value_parser = ["csv", "json", "mbox"])]
        format: Option<String>,
        /// Also emit inferred per-field type metadata alongside the rows.
        #[arg(long)]
        meta: bool,
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
        Commands::Data { file, format, meta } => cli::data(file, format, meta, json),
        Commands::Skill { install_claude, user } => cli::skill(install_claude, user),
    }
}
