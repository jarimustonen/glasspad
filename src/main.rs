// The tabular data parsers and security primitives live in the library crate
// (`glasspad::{data,security}`). The binary reaches them through the lib: the
// artifact host uses `security::token` for per-response nonces, and the optional
// `glasspad data` subcommand uses `glasspad::data` to parse the legacy CSV / JSON
// / mbox formats on demand. The section-DSL renderer that once consumed them was
// removed in Wave 5 / Phase 6.
mod artifact_host;
mod build;
mod cli;
mod hosted;
mod pidfile;
mod server;
mod submissions;

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "glasspad",
    about = "AI scratchpad for rich HTML artifact views",
    // Disable clap's built-in `--version` (it short-circuits during parse and
    // would ignore `--json`, emitting plain text). We wire our own `-V/--version`
    // below so it routes through `cli::version` and honors `--json` like every
    // other command. A bare `glasspad` (no subcommand, no flag) prints help.
    disable_version_flag = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Print the installed version and exit (honors --json; same output as the
    /// `version` subcommand). Top-level only, mirroring clap's built-in flag.
    #[arg(short = 'V', long = "version")]
    version: bool,

    /// Emit machine-readable JSON (stable, versioned envelopes; errors to stderr).
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Serve a directory live as a space (scan + watch + SSE). With no directory,
    /// serves the built-in fixtures. Runs until killed.
    Serve {
        /// Directory to serve as a space (`.html` served verbatim; `.md`/`.markdown`
        /// rendered server-side). Omit to serve only the built-in fixtures.
        dir: Option<PathBuf>,
        /// TCP port on 127.0.0.1 (1-65535). Precedence (AI-first §8): this flag >
        /// $GLASSPAD_PORT > the built-in default (3000).
        #[arg(short, long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
    },
    /// Build a one-artifact space from a single file and serve it live.
    Create {
        /// The HTML file to serve (fragment or full document — auto-detected).
        file: PathBuf,
        /// Space name (default: the file stem). Must match the space grammar.
        #[arg(long)]
        name: Option<String>,
        /// TCP port on 127.0.0.1 (1-65535). Precedence (AI-first §8): this flag >
        /// $GLASSPAD_PORT > the built-in default (3000).
        #[arg(short, long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
    },
    /// Render a markdown file through a reusable template and serve it live.
    ///
    /// The markdown body is rendered to HTML server-side and spliced into the
    /// template's single `{{content}}` placeholder; the result is hosted as the
    /// artifact body (same sandbox/CSP as `create`). The template governs only
    /// the body — never the trusted shell, CSP, or sandbox.
    Render {
        /// The markdown file to render.
        file: PathBuf,
        /// Template reference: a built-in name (`prose` [default] / `dashboard`)
        /// or a path to a template HTML file containing one `{{content}}` slot.
        #[arg(long)]
        template: Option<String>,
        /// Space name (default: the file stem). Must match the space grammar.
        #[arg(long)]
        name: Option<String>,
        /// TCP port on 127.0.0.1 (1-65535). Precedence (AI-first §8): this flag >
        /// $GLASSPAD_PORT > the built-in default (3000).
        #[arg(short, long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
    },
    /// Statically render a space directory to self-contained HTML files (no
    /// server, no bind). Reuses the same scanner + wrap seam `serve` uses, writing
    /// the wrapped pages to `<out>` for an offline docsite / preview transport.
    Build {
        /// The space directory to render (same scan + validation as `serve`).
        space: PathBuf,
        /// Output directory for the rendered files. Created if absent; must be
        /// empty unless `--force`.
        out: PathBuf,
        /// Reference the base libs at the absolute `/_gp/v1/…` server path instead
        /// of bundling them (default: self-contained — bundle + reference them
        /// relatively so the output works offline).
        #[arg(long)]
        shared_libs: bool,
        /// Write into a non-empty output directory (may overwrite existing files).
        #[arg(long)]
        force: bool,
        /// Validate + plan the build and print what would be written, without
        /// writing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run the hosted share server (public bind, API-key ingest, capability-slug
    /// public read). A separate run mode from `serve`: it binds the given public
    /// address and does NOT use the loopback DNS-rebinding guard. Runs until killed.
    HostServe {
        /// Public bind address, e.g. 0.0.0.0:8080. Explicit — never defaulted to a
        /// routable interface.
        #[arg(long)]
        bind: SocketAddr,
        /// Canonical public origin for the artifact CSP + returned URLs, e.g.
        /// https://pad.example.com (scheme://host[:port], no path).
        #[arg(long)]
        public_host: String,
        /// Operator API-key file: `<tenant>:<key>` lines. Fail-closed if
        /// missing/empty/malformed.
        #[arg(long)]
        api_key_file: PathBuf,
        /// Storage root for published pages.
        #[arg(long)]
        store: PathBuf,
        /// Days before an immutable page is garbage-collected.
        #[arg(long, default_value_t = 90, value_parser = clap::value_parser!(i64).range(1..))]
        retention_days: i64,
    },
    /// Publish one page to a hosted share server and print its slug + URL.
    ///
    /// Config precedence: flag > $GLASSPAD_SERVER / $GLASSPAD_API_KEY > the file
    /// ~/.config/glasspad/config.yaml. The API key is never printed.
    Publish {
        /// The file to publish (HTML by default; markdown with --markdown).
        file: PathBuf,
        /// Hosted server base URL, e.g. https://pad.example.com.
        #[arg(long)]
        server: Option<String>,
        /// Bearer API key for ingest auth.
        #[arg(long)]
        api_key: Option<String>,
        /// Treat the file as markdown and render it server-side.
        #[arg(long)]
        markdown: bool,
        /// With --markdown: a built-in template name (prose/dashboard) or a path to
        /// a template file with one {{content}} slot.
        #[arg(long)]
        template: Option<String>,
        /// Override the resolved display title.
        #[arg(long)]
        title: Option<String>,
        /// Optional idempotency key: a repeat publish with the same key returns the
        /// first page (HTTP 200) instead of minting a new one — exactly-once for a
        /// deterministic caller across a lost receipt.
        #[arg(long)]
        idempotency_key: Option<String>,
        /// Do not open the published URL in a browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Publish a whole SPACE (a directory of linked .html and/or .md artifacts) into
    /// one hosted namespace /p/{slug}/… with in-space nav + relative links across pages.
    ///
    /// The directory is scanned locally with the same rules as `serve`/`build`
    /// (slug grammar, reserved names, symlink/traversal rejection, size caps, MIME,
    /// glasspad.yaml nav/title/template); `.md` pages are rendered server-side, then the
    /// resulting pages are sent as one bundle. Config precedence mirrors `publish`. The
    /// API key is never printed.
    PublishSpace {
        /// The directory to publish (a space of .html and/or .md pages).
        dir: PathBuf,
        /// Hosted server base URL, e.g. https://pad.example.com.
        #[arg(long)]
        server: Option<String>,
        /// Bearer API key for ingest auth.
        #[arg(long)]
        api_key: Option<String>,
        /// Optional stable space key: a re-publish with the same key updates the
        /// space IN PLACE at the same slug/URL (idempotent hosting of a docsite that
        /// changes over time). Absent → a fresh slug each publish.
        #[arg(long)]
        space_key: Option<String>,
        /// Do not open the published space URL in a browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Re-render an already-published hosted page in response to a submission (B2
    /// multi-round). POSTs a new body to the page's round endpoint (API-key auth,
    /// owner-scoped); the server swaps the live page's content in place for every
    /// connected viewer. Prints `{slug, round, content_version}`. Config mirrors
    /// `publish` (flag > $GLASSPAD_SERVER/$GLASSPAD_API_KEY > config file).
    PushRound {
        /// The page slug to re-render (the `{slug}` in `/p/{slug}/`).
        slug: String,
        /// The new round's source file (HTML by default; markdown with --markdown).
        file: PathBuf,
        /// Hosted server base URL, e.g. https://pad.example.com.
        #[arg(long)]
        server: Option<String>,
        /// Bearer API key for the owning tenant (required).
        #[arg(long)]
        api_key: Option<String>,
        /// Treat the file as markdown, rendered server-side (optionally --template).
        #[arg(long)]
        markdown: bool,
        /// With --markdown: a built-in template name (prose/dashboard) or a template
        /// file path with one {{content}} slot.
        #[arg(long)]
        template: Option<String>,
    },
    /// Block on the next user submission an interactive artifact sent back, then
    /// print it (the return channel's agent-facing surface).
    ///
    /// Run it BACKGROUNDED: it rides a server-side long-poll and returns the human's
    /// answer as its result (stdout = one compact JSON submission per line under
    /// `--json` the full `{submissions, cursor, timed_out}` envelope). On timeout it
    /// exits 3 with a distinct "no submission" result so you can re-arm from the
    /// returned cursor. Mode mirrors `publish`: a `--server` (or $GLASSPAD_SERVER)
    /// targets the hosted server (`<slug>` = page slug, API-key auth); with none it
    /// targets the local `serve` on `--port` (`<slug>` = space name, no auth).
    AwaitSubmission {
        /// The page slug (hosted) or space name (loopback) to await input for.
        slug: String,
        /// Only return submissions with an id greater than this cursor (default 0).
        #[arg(long, default_value_t = 0)]
        since: u64,
        /// Seconds to hold before returning a "timed-out" result (1..=300).
        #[arg(long, default_value_t = 30)]
        timeout: u64,
        /// Hosted server base URL (selects hosted mode), e.g. https://pad.example.com.
        #[arg(long)]
        server: Option<String>,
        /// Bearer API key for the hosted read (required in hosted mode).
        #[arg(long)]
        api_key: Option<String>,
        /// Loopback port. Passing it forces loopback mode (targets the local
        /// `serve`) even when a hosted server is configured.
        #[arg(short, long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
        /// Consume the server-push SSE stream instead of the long-poll. An opt-in
        /// transport for watching many pages or sub-second streaming; the plain
        /// long-poll stays the default. Returns the first submission then exits (add
        /// --follow to keep streaming). Honors --timeout as the overall hold.
        #[arg(long)]
        stream: bool,
        /// With --stream: keep the stream open and print every submission as it lands
        /// (until --timeout), rather than returning after the first.
        #[arg(long, requires = "stream")]
        follow: bool,
    },
    /// Stop the running loopback server (`serve` / `create` / `render`).
    ///
    /// Reads the pid file at ~/.glasspad/server.pid (override with $GLASSPAD_PID_FILE)
    /// and sends SIGTERM, which the server traps to remove its pid file and exit
    /// cleanly. A stale pid file (recorded process already dead) is cleaned and
    /// reported as "no running server" rather than treated as still-running. Targets
    /// a LOCAL process only — no network call, so the loopback Host guard is untouched.
    Stop,
    /// Open a served space's URL in the browser.
    Open {
        /// Space name (the `{space}` in `/{space}/`).
        space: String,
        /// TCP port on 127.0.0.1 (1-65535). Precedence (AI-first §8): this flag >
        /// $GLASSPAD_PORT > the built-in default (3000).
        #[arg(short, long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
        /// Print the URL without launching a browser (pipe-friendly).
        #[arg(long)]
        no_browser: bool,
    },
    /// Parse a legacy tabular file (CSV / JSON / mbox) to JSON rows on stdout.
    ///
    /// A standalone helper for the old data formats: it parses the file (bounded
    /// by a 50 MB read cap plus the parsers' own row/column limits) and prints the
    /// rows as JSON (the data channel), so a caller can reuse those inputs when
    /// authoring an HTML artifact. It never starts a server.
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
    /// Report the installed CLI version (for version-gating; use --json for a
    /// machine-readable envelope). Mirrors the built-in `--version` / `-V` flag.
    Version,
    /// Output or install the CLI's companion skill (its operating manual).
    Skill {
        /// Install the skill file. Project-level by default, --user for the home dir.
        /// (`--install` is the preferred spelling now that the install dual-homes
        /// beyond Claude Code; `--install-claude` is kept as a compatibility alias.)
        #[arg(long = "install-claude", alias = "install")]
        install_claude: bool,
        /// Use with --install: install under the home dir instead of the project
        #[arg(long, requires = "install_claude")]
        user: bool,
        /// With --install: which agent skill dir(s) to install into — `claude`
        /// (~/.claude or ./.claude), `pi` (~/.pi/agent or ./.pi), or `all` (dual-home
        /// both). Omit to dual-home (the default). Optional so that passing it
        /// without --install is a usage error rather than a silent no-op.
        #[arg(long, value_enum, requires = "install_claude")]
        agent: Option<cli::SkillAgent>,
    },
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let json = args.json;

    // The `-V/--version` flag and the `version` subcommand are one entry point:
    // both route through `cli::version`, so all three spellings honor `--json`.
    if args.version {
        cli::version(json);
        return;
    }

    match args.command {
        Some(Commands::Serve { dir, port }) => {
            cli::serve(dir, cli::resolve_port(port, json), json).await
        }
        Some(Commands::Create { file, name, port }) => {
            cli::create(file, name, cli::resolve_port(port, json), json).await
        }
        Some(Commands::Render {
            file,
            template,
            name,
            port,
        }) => cli::render(file, template, name, cli::resolve_port(port, json), json).await,
        Some(Commands::Build {
            space,
            out,
            shared_libs,
            force,
            dry_run,
        }) => cli::build(space, out, shared_libs, force, dry_run, json),
        Some(Commands::HostServe {
            bind,
            public_host,
            api_key_file,
            store,
            retention_days,
        }) => cli::host_serve(bind, public_host, api_key_file, store, retention_days, json).await,
        Some(Commands::Publish {
            file,
            server,
            api_key,
            markdown,
            template,
            title,
            idempotency_key,
            no_open,
        }) => {
            cli::publish(
                file,
                server,
                api_key,
                markdown,
                template,
                title,
                idempotency_key,
                json,
                no_open,
            )
            .await
        }
        Some(Commands::PublishSpace {
            dir,
            server,
            api_key,
            space_key,
            no_open,
        }) => cli::publish_space(dir, server, api_key, space_key, json, no_open).await,
        Some(Commands::PushRound {
            slug,
            file,
            server,
            api_key,
            markdown,
            template,
        }) => cli::push_round(slug, file, server, api_key, markdown, template, json).await,
        Some(Commands::AwaitSubmission {
            slug,
            since,
            timeout,
            server,
            api_key,
            port,
            stream,
            follow,
        }) => {
            cli::await_submission(
                slug, since, timeout, server, api_key, port, stream, follow, json,
            )
            .await
        }
        Some(Commands::Stop) => cli::stop(json),
        Some(Commands::Open {
            space,
            port,
            no_browser,
        }) => cli::open(space, cli::resolve_port(port, json), json, no_browser),
        Some(Commands::Data { file, format, meta }) => cli::data(file, format, meta, json),
        Some(Commands::Version) => cli::version(json),
        Some(Commands::Skill {
            install_claude,
            user,
            agent,
        }) => cli::skill(install_claude, user, agent, json),
        // `arg_required_else_help` covers a bare `glasspad`; this reaches only a
        // no-subcommand invocation that still carried an arg (e.g. `glasspad
        // --json`). Print help and exit non-zero (a usage error, like clap's).
        None => {
            let _ = Cli::command().print_help();
            std::process::exit(2);
        }
    }
}
