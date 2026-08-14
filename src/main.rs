// The tabular data parsers and security primitives live in the library crate
// (`glasspad::{data,security}`). The binary reaches them through the lib: the
// artifact host uses `security::token` for per-response nonces, and the optional
// `glasspad data` subcommand uses `glasspad::data` to parse the legacy CSV / JSON
// / mbox formats on demand. The section-DSL renderer that once consumed them was
// removed in Wave 5 / Phase 6.
mod artifact_host;
mod build;
mod cli;
mod config;
mod favicon;
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
    /// Advanced: statically render a space directory to self-contained HTML files
    /// (no server, no bind). Reuses the same scanner + wrap seam the loopback host
    /// uses, writing the wrapped pages to `<out>` for an offline docsite / preview
    /// transport, or to inspect the raw wrapped HTML while debugging.
    Build {
        /// The space directory to render (same scan + validation as `publish`).
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
    /// Operator: run the hosted share server (public bind, API-key ingest,
    /// capability-slug public read) that `publish` (hosted target) uploads TO. A
    /// separate run mode from loopback: it binds the given public address and does
    /// NOT use the loopback DNS-rebinding guard. Runs until killed.
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
    /// Publish markdown (or HTML) and get back a URL — THE default verb.
    ///
    /// `<path>` is a `.md`/`.markdown`/`.html` FILE (a one-page space) or a
    /// DIRECTORY of them (an N-page space). Markdown is the standard input and is
    /// rendered automatically. Where it lands is resolved from config (not a flag):
    /// a `target` of `loopback` (serve on 127.0.0.1 with live reload, the zero-config
    /// default) or `hosted` (upload to a share server, return a /p/<slug>/ URL).
    ///
    /// Config precedence is per-key, first that sets a key wins: repo-local
    /// `.glasspad.yaml` (found by walking up from the CWD) > home
    /// ~/.config/glasspad/config.yaml > built-in default (loopback). Flags and the
    /// matching env vars still override the merged config. The API key is never printed.
    Publish {
        /// The file (.md/.markdown/.html) or directory to publish.
        path: PathBuf,
        /// Override the resolved target for this publish: `loopback` or `hosted`.
        /// Precedence: this flag > $GLASSPAD_TARGET > config `target:` > loopback.
        #[arg(long)]
        target: Option<String>,
        /// Hosted server base URL, e.g. https://pad.example.com (hosted target).
        #[arg(long)]
        server: Option<String>,
        /// Bearer API key for hosted ingest auth (hosted target).
        #[arg(long)]
        api_key: Option<String>,
        /// Template for markdown pages: a built-in name (prose [default] / dashboard)
        /// or a path to a template file with one {{content}} slot. Defaults to the
        /// config `template:` value, else `prose`.
        #[arg(long)]
        template: Option<String>,
        /// Override the resolved space/page title (hosted target).
        #[arg(long)]
        title: Option<String>,
        /// Stable space key (hosted target): a re-publish with the same key updates
        /// the space IN PLACE at the same /p/<slug>/ URL (idempotent). Defaults to
        /// the config `space_key:` value. Absent → a fresh slug each publish.
        #[arg(long)]
        space_key: Option<String>,
        /// Loopback TCP port (loopback target). Precedence (AI-first §8): this flag >
        /// $GLASSPAD_PORT > the built-in default (3000).
        #[arg(short, long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
        /// Do not open the resulting URL in a browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Advanced: manage the loopback live-reload server directly (serve / open /
    /// stop). The standard flow is `publish`; reach for these only for explicit
    /// loopback control. See `glasspad loopback --help`.
    Loopback {
        #[command(subcommand)]
        cmd: LoopbackCmd,
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
    /// Drain a hosted page's return-channel backlog in one shot — every user
    /// submission already stored for `<slug>`, without blocking.
    ///
    /// The returning-agent companion to `await-submission`: where that BLOCKS for
    /// the next answer inside a live session, this does a single plain poll and
    /// returns immediately with everything persisted so far (stdout = one compact
    /// JSON submission per line; `--json` the full `{submissions, cursor}` envelope).
    /// Use it when you published a page, walked away, and came back: submissions
    /// persist server-side for the retention window, so `--since 0` (the default)
    /// drains the whole retained backlog. Hosted-only + API-key-authenticated +
    /// owner-scoped (a slug your key does not own is an opaque `no_such_page`);
    /// config mirrors `publish` (--server / $GLASSPAD_SERVER, --api-key /
    /// $GLASSPAD_API_KEY > config file).
    Submissions {
        /// The page slug (the `{slug}` in `/p/{slug}/`) to drain submissions for.
        slug: String,
        /// Only return submissions with an id greater than this cursor (default 0 =
        /// the whole retained backlog).
        #[arg(long, default_value_t = 0)]
        since: u64,
        /// Hosted server base URL (required), e.g. https://pad.example.com.
        #[arg(long)]
        server: Option<String>,
        /// Bearer API key for the owning tenant (required).
        #[arg(long)]
        api_key: Option<String>,
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

/// Advanced loopback-server management. Grouped under `glasspad loopback` and
/// discoverable via `--help` only — the standard flow is `publish` (which, for a
/// loopback target, folds serve + open into one step).
#[derive(Subcommand)]
enum LoopbackCmd {
    /// Serve a file or directory live on 127.0.0.1 (scan + watch + SSE), blocking
    /// until killed. With no path, serves the built-in fixtures. `publish` is the
    /// standard entry point; use this for explicit loopback control.
    Serve {
        /// A directory (`.html` served verbatim; `.md`/`.markdown` rendered), a
        /// single file, or omitted (serve only the built-in fixtures).
        path: Option<PathBuf>,
        /// Template for a single markdown file: a built-in name (prose/dashboard)
        /// or a path to a template file with one {{content}} slot.
        #[arg(long)]
        template: Option<String>,
        /// Space name for a single file (default: the file stem).
        #[arg(long)]
        name: Option<String>,
        /// TCP port on 127.0.0.1 (1-65535). Precedence (AI-first §8): this flag >
        /// $GLASSPAD_PORT > the built-in default (3000).
        #[arg(short, long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
        /// SECURITY-SENSITIVE opt-in: also serve on this LAN address so other devices
        /// on the same local network can load the space. Pass the explicit private LAN
        /// IPv4 other devices reach this machine at, e.g. `--bind 192.168.1.50`.
        /// Loopback stays bound; only this one IP is added to the DNS-rebinding
        /// allowlist. A trusted-LAN convenience carrying NO API key — never a public
        /// bind: hostnames (DNS-rebinding risk), wildcard (0.0.0.0), IPv6, and public
        /// IPs are all refused; only RFC1918 / link-local / CGNAT ranges are accepted.
        /// Precedence: this flag > $GLASSPAD_BIND > `bind:` in your HOME config (a
        /// repo-local .glasspad.yaml `bind:` is ignored). Omitted → loopback-only.
        #[arg(long, value_name = "LAN-IPV4")]
        bind: Option<String>,
        /// Open the served URL in a browser after binding.
        #[arg(long)]
        open: bool,
    },
    /// Open a served space's loopback URL in the browser.
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
    /// Stop the running loopback server.
    ///
    /// Reads the pid file at ~/.glasspad/server.pid (override with $GLASSPAD_PID_FILE)
    /// and sends SIGTERM, which the server traps to remove its pid file and exit
    /// cleanly. A stale pid file (recorded process already dead) is cleaned and
    /// reported as "no running server" rather than treated as still-running. Targets
    /// a LOCAL process only — no network call, so the loopback Host guard is untouched.
    Stop,
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
            path,
            target,
            server,
            api_key,
            template,
            title,
            space_key,
            port,
            no_open,
        }) => {
            cli::publish(
                path, target, server, api_key, template, title, space_key, port, no_open, json,
            )
            .await
        }
        Some(Commands::Loopback { cmd }) => match cmd {
            LoopbackCmd::Serve {
                path,
                template,
                name,
                port,
                bind,
                open,
            } => {
                cli::loopback_serve(
                    path,
                    template,
                    name,
                    cli::resolve_port(port, json),
                    bind,
                    open,
                    json,
                )
                .await
            }
            LoopbackCmd::Open {
                space,
                port,
                no_browser,
            } => cli::open(space, cli::resolve_port(port, json), json, no_browser),
            LoopbackCmd::Stop => cli::stop(json),
        },
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
        Some(Commands::Submissions {
            slug,
            since,
            server,
            api_key,
        }) => cli::submissions(slug, since, server, api_key, json).await,
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
