use super::runtime::*;
use super::*;

// --- host-serve (hosted share server) -------------------------------------

/// Recognize native and IPv4-mapped wildcard addresses without requiring the
/// operating system to support binding every equivalent socket representation.
pub(super) fn is_host_wildcard_bind(bind: SocketAddr) -> bool {
    bind.ip().to_canonical().is_unspecified()
}

/// `glasspad host-serve --bind <ip:port> [--public-host <origin>] --api-key-file
/// <path> --store <dir> [--retention-days <n>]` runs the long-lived hosted share
/// server. A loopback port of 0 is bound once and its OS-assigned address is
/// reported; without an explicit public host, that real address becomes the public
/// origin. Wildcard binds are permitted for public deployments and produce a loud
/// warning naming the actual bound port.
pub async fn host_serve(
    bind: SocketAddr,
    public_host: Option<String>,
    api_key_file: PathBuf,
    store: PathBuf,
    retention_days: i64,
    json: bool,
) {
    let bind_ip = bind.ip().to_canonical();
    let wildcard_bind = is_host_wildcard_bind(bind);
    if public_host.is_none() && !bind_ip.is_loopback() {
        exit_error(
            json,
            1,
            "missing_public_host",
            "--public-host is required unless --bind names a loopback address",
            Some(&bind.to_string()),
            None,
        );
    }

    // Validate an explicit public origin before any I/O. When absent, hosted::run
    // derives it only after the loopback listener has obtained its real port.
    let public_origin = public_host.as_deref().map(|raw| {
        hosted::validate_public_origin(raw)
            .unwrap_or_else(|msg| exit_error(json, 1, "invalid_public_host", &msg, Some(raw), None))
    });

    // Load the operator key file — fail-closed: the server never comes up with an
    // ingest surface no key (or any key) can authenticate.
    let keys = match KeyTable::load(&api_key_file) {
        Ok(k) => Arc::new(k),
        Err(e) => {
            let (code, exit) = match e {
                KeyFileError::Io(_) => ("api_key_file_unreadable", 2),
                _ => ("invalid_api_key_file", 1),
            };
            exit_error(
                json,
                exit,
                code,
                &e.to_string(),
                Some(&api_key_file.display().to_string()),
                None,
            );
        }
    };
    let key_count = keys.len();

    let config = HostedConfig {
        bind,
        public_origin,
        store_root: store,
        retention_days,
    };

    let handle = match hosted::run(config, keys).await {
        Ok(h) => h,
        Err(msg) => exit_error(json, 2, "host_start_failed", &msg, None, None),
    };
    let local = handle
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| bind.to_string());
    let wildcard_warning = wildcard_bind.then(|| warn_host_wildcard(&local));
    emit_host_serving(
        json,
        &local,
        &handle.public_origin,
        handle.pages,
        key_count,
        retention_days,
        wildcard_warning.as_deref(),
    );

    if let Err(e) = handle.serve().await {
        exit_error(
            json,
            2,
            "serve_failed",
            &format!("hosted server stopped with an error: {e}"),
            None,
            None,
        );
    }
}

/// Emit the loud wildcard-bind warning only after the listener is held, so an
/// ephemeral bind names the OS-assigned port rather than the requested port 0.
pub(super) fn warn_host_wildcard(bind: &str) -> String {
    let warning = format!(
        "WILDCARD BIND: glasspad host-serve is reachable on EVERY NETWORK INTERFACE; \
         listening on {bind}"
    );
    eprintln!("⚠️  {warning}");
    warning
}

/// Startup envelope for `host-serve` (mirrors [`emit_serving`]): a long-running
/// announcement, not a terminal result. `--json` → stdout; text → stderr.
pub(super) fn emit_host_serving(
    json: bool,
    bind: &str,
    public_origin: &str,
    pages: usize,
    keys: usize,
    retention_days: i64,
    warning: Option<&str>,
) {
    if json {
        let warnings: Vec<&str> = warning.into_iter().collect();
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "serving": true,
            "mode": "hosted",
            "bind": bind,
            "public_host": public_origin,
            "ingest": format!("{public_origin}/api/v1/pages"),
            "mount": hosted::MOUNT,
            "pages": pages,
            "api_keys": keys,
            "retention_days": retention_days,
            "warnings": warnings,
        });
        emit_json_line(&payload);
    } else {
        eprintln!(
            "glasspad hosted share server on {bind} (public {public_origin}); \
             {pages} page(s), {keys} key(s), {retention_days}d retention"
        );
    }
}
