use super::publish::{
    resolve_api_key, resolve_publish_config, resolve_server, resolve_setting, server_is_loopback,
};
use super::runtime::*;
use super::*;

// --- submissions (drain the return-channel backlog) -----------------------

/// `glasspad submissions <slug> [--since <cursor>] [--server <url>]
/// [--api-key <key>] [--json]` — drain the whole return-channel backlog a hosted
/// page accumulated while no agent was listening.
///
/// This is the **returning-agent** surface (companion to `await-submission`):
/// where `await-submission` *blocks* for the next answer inside a live session,
/// `submissions` polls the durable store and returns everything persisted for
/// `<slug>` since `--since` (default `0` = the whole retained backlog). A page that
/// was published and forgotten keeps every human answer for the server's retention
/// window, so an agent that comes back later drains it here. The server caps each
/// read (`MAX_LIST`), so this **pages** through the backlog — advancing the cursor
/// until a fetch comes back empty — rather than silently returning only the first
/// page. A non-destructive read: it does not delete or acknowledge, so re-running
/// with the same `--since` returns the same records.
///
/// Hosted-only: the return-channel store and its per-tenant scoping live on the
/// hosted server, so a `--server` (flag / `$GLASSPAD_SERVER` / config) is required
/// and the read is API-key-authenticated + owner-scoped — a slug the key's tenant
/// does not own is an opaque `no_such_page` (never a cross-tenant read).
pub async fn submissions(
    slug: String,
    since: u64,
    server: Option<String>,
    api_key: Option<String>,
    json: bool,
) {
    // The slug obeys the same grammar the hosted router enforces (fail before any
    // network round-trip, per the AI-first CLI contract).
    if !artifact_host::valid_space(&slug) {
        exit_error(
            json,
            1,
            "invalid_slug",
            "slug must be lowercase [a-z0-9-], start alphanumeric, ≤64 chars, and not be reserved",
            Some(&slug),
            None,
        );
    }

    let cfg = resolve_publish_config(json);
    // Hosted-only: the backlog and its per-tenant scope live on the hosted server.
    // `resolve_server` exits with `missing_server` when none is configured, which is
    // the right failure — there is no loopback backlog to drain.
    let server = resolve_server(server, &cfg, json);
    let api_key = resolve_api_key(api_key, &cfg, json);
    let mut warnings: Vec<String> = Vec::new();
    if server.starts_with("http://") && !server_is_loopback(&server) {
        let w = "draining over plaintext http:// to a non-local host sends the API key in the \
                 clear; prefer https://";
        // Under --json the warning belongs in the envelope's `warnings` array, not on
        // stderr as unstructured text (AI-first CLI contract); otherwise print it.
        if json {
            warnings.push(w.to_string());
        } else {
            eprintln!("warning: {w}");
        }
    }

    let base = server.trim_end_matches('/');
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => exit_error(json, 2, "client_init_failed", &e.to_string(), None, None),
    };

    // Page through the backlog: each read returns at most `MAX_LIST` rows and a
    // cursor; keep fetching from the cursor until a page comes back empty. The
    // cursor is strictly monotonic server-side, so this always terminates and never
    // re-reads a row. (A steady live writer would keep feeding new rows; we stop the
    // moment a fetch returns nothing, so at worst we also drain rows that arrived
    // mid-scan — the intended behaviour for a "give me everything" drain.)
    let mut cursor = since;
    let mut all: Vec<serde_json::Value> = Vec::new();
    loop {
        let url = format!("{base}/api/v1/pages/{slug}/submissions?since={cursor}");
        let resp = match client.get(&url).bearer_auth(&api_key).send().await {
            Ok(r) => r,
            Err(e) => exit_error(
                json,
                2,
                "request_failed",
                &format!("cannot reach {url}: {e}"),
                None,
                None,
            ),
        };

        let status = resp.status();
        let payload: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
        if !status.is_success() {
            let msg = payload
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("the server rejected the read");
            let code = payload
                .get("error")
                .and_then(|e| e.get("code"))
                .and_then(|c| c.as_str())
                .unwrap_or("read_rejected");
            let exit = if status.is_client_error() { 1 } else { 2 };
            exit_error(
                json,
                exit,
                code,
                &format!("{msg} (HTTP {})", status.as_u16()),
                None,
                None,
            );
        }

        // A 2xx with a body that is not the documented envelope (a proxy error page,
        // truncated JSON, wrong shape) must NOT be silently reported as "no backlog":
        // require a `submissions` array, else fail as a protocol error (exit 2).
        let Some(page) = payload.get("submissions").and_then(|s| s.as_array()) else {
            exit_error(
                json,
                2,
                "invalid_server_response",
                &format!(
                    "server returned HTTP {} but no submissions array",
                    status.as_u16()
                ),
                None,
                None,
            );
        };
        let next = payload.get("cursor").and_then(|c| c.as_u64());

        if page.is_empty() {
            // Drained: adopt the server's terminal cursor when it gave one.
            if let Some(c) = next {
                cursor = c;
            }
            break;
        }

        // Non-JSON mode streams each row as it lands (so a huge backlog pipes without
        // buffering); JSON mode accumulates for one envelope at the end.
        for s in page {
            if json {
                all.push(s.clone());
            } else {
                println!("{}", serde_json::to_string(s).unwrap_or_default());
            }
        }

        // Advance strictly; a non-advancing/absent cursor would loop forever, so stop.
        match next {
            Some(c) if c > cursor => cursor = c,
            _ => break,
        }
    }

    if json {
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "submissions": all,
            "cursor": cursor,
            "warnings": warnings,
        }));
    } else {
        eprintln!("drained backlog for '{slug}' (final cursor {cursor})");
    }
    // A drain is informational: exit 0 whether or not the backlog was empty (an
    // empty backlog is a valid answer, not an error — unlike `await-submission`'s
    // timeout, which has a distinct exit 3).
}

// --- await-submission (return-channel client) -----------------------------

/// `glasspad await-submission <slug> [--since <cursor>] [--timeout <secs>]
/// [--server <url>] [--api-key <key>] [--port <port>] [--json]` — block on the
/// next user submission an interactive artifact sent back, then print it.
///
/// This is the **primary agent-facing surface** of the return channel (design
/// A3): the agent runs it **backgrounded** and gets the human's answer as the
/// command's return value — no polling loop, no cursor bookkeeping. It rides a
/// **server-side long-poll** (`…/submissions/wait`), so it wastes no requests
/// while nothing arrives, and it always returns within `--timeout` with a
/// **distinct** "timed-out, no submission" result (exit code 3) so a backgrounded
/// caller can re-arm from the returned `cursor` or give up.
///
/// Mode selection: an explicit `--server` selects the **hosted** server (API-key
/// auth, `<slug>` = the page slug); an explicit `--port` (a loopback-only concept)
/// selects the **loopback** `serve` process even when a hosted server is configured
/// (`<slug>` = the space name, no auth — loopback only); with neither flag it uses
/// `$GLASSPAD_SERVER`/config if set, else loopback on the default port.
#[allow(clippy::too_many_arguments)]
pub async fn await_submission(
    slug: String,
    since: u64,
    timeout: u64,
    server: Option<String>,
    api_key: Option<String>,
    port: Option<u16>,
    stream: bool,
    follow: bool,
    json: bool,
) {
    // The slug/space addressing token obeys the same grammar the router enforces.
    if !artifact_host::valid_space(&slug) {
        exit_error(
            json,
            1,
            "invalid_slug",
            "slug must be lowercase [a-z0-9-], start alphanumeric, ≤64 chars, and not be reserved",
            Some(&slug),
            None,
        );
    }
    let timeout = timeout.clamp(1, crate::submissions::MAX_WAIT_SECS);

    let cfg = resolve_publish_config(json);
    // Mode selection: an explicit `--server` forces hosted; an explicit `--port`
    // (a loopback-only concept) forces loopback even when a hosted server is
    // configured; otherwise fall back to the configured/env server, else loopback.
    let server_flag = server.filter(|s| !s.trim().is_empty());
    let server = match (server_flag, port) {
        (Some(s), _) => Some(s),
        (None, Some(_)) => None,
        (None, None) => resolve_setting(None, "GLASSPAD_SERVER", cfg.server.clone()),
    };

    // Build the wait URL + optional bearer per mode.
    let (url, bearer) = match server {
        Some(server) => {
            let api_key = resolve_api_key(api_key, &cfg, json);
            if server.starts_with("http://") && !server_is_loopback(&server) {
                eprintln!(
                    "warning: awaiting over plaintext http:// to a non-local host sends the API \
                     key in the clear; prefer https://"
                );
            }
            let base = server.trim_end_matches('/');
            let url = if stream {
                format!("{base}/api/v1/pages/{slug}/submissions/stream?since={since}")
            } else {
                format!(
                    "{base}/api/v1/pages/{slug}/submissions/wait?since={since}&timeout={timeout}"
                )
            };
            (url, Some(api_key))
        }
        None => {
            // Loopback: target the local `serve` process on the resolved port.
            let port = resolve_port(port, json);
            let url = if stream {
                format!("http://127.0.0.1:{port}/{slug}/_gp/submissions/stream?since={since}")
            } else {
                format!(
                    "http://127.0.0.1:{port}/{slug}/_gp/submissions/wait?since={since}&timeout={timeout}"
                )
            };
            (url, None)
        }
    };

    // The HTTP timeout must outlast the server-side long-poll so the *server*
    // returns the "timed out" result first (rather than the client aborting).
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(timeout + 15))
        .build()
    {
        Ok(c) => c,
        Err(e) => exit_error(json, 2, "client_init_failed", &e.to_string(), None, None),
    };
    let mut request = client.get(&url);
    if let Some(k) = &bearer {
        request = request.bearer_auth(k);
    }
    // SSE requests advertise the media type so a proxy never buffers/transcodes.
    if stream {
        request = request.header(reqwest::header::ACCEPT, "text/event-stream");
    }
    let resp = match request.send().await {
        Ok(r) => r,
        Err(e) => exit_error(
            json,
            2,
            "request_failed",
            &format!("cannot reach {url}: {e}"),
            None,
            None,
        ),
    };

    // SSE transport (A2): consume the server-push stream instead of the long-poll.
    if stream {
        consume_submission_stream(resp, since, timeout, follow, json).await;
    }

    let status = resp.status();
    let payload: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let msg = payload
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("the server rejected the wait");
        let code = payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("await_rejected");
        let exit = if status.is_client_error() { 1 } else { 2 };
        exit_error(
            json,
            exit,
            code,
            &format!("{msg} (HTTP {})", status.as_u16()),
            None,
            None,
        );
    }

    let submissions = payload
        .get("submissions")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    let cursor = payload
        .get("cursor")
        .and_then(|c| c.as_u64())
        .unwrap_or(since);
    let timed_out = payload
        .get("timed_out")
        .and_then(|t| t.as_bool())
        .unwrap_or(submissions.is_empty());

    if json {
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "timed_out": timed_out,
            "submissions": submissions,
            "cursor": cursor,
            "warnings": [],
        }));
    } else if timed_out {
        eprintln!("no submission before the {timeout}s timeout (re-arm from cursor {cursor})");
    } else {
        // stdout is the data channel: one compact JSON submission per line, so a
        // backgrounded caller can read the answer directly.
        for s in &submissions {
            println!("{}", serde_json::to_string(s).unwrap_or_default());
        }
        eprintln!(
            "received {} submission(s) (next cursor {cursor})",
            submissions.len()
        );
    }
    // Exit code encodes the outcome: 0 = at least one submission, 3 = timed out with
    // none (a distinct, non-error status so a backgrounded agent can branch on it).
    std::process::exit(if timed_out { 3 } else { 0 });
}

/// Consume the return-channel **SSE stream** (A2 transport) and diverge with the same
/// exit-code contract as the long-poll path: `0` once at least one submission is
/// printed, `3` if the `timeout` elapses (or the server closes the stream) with none.
///
/// Each `submission` event's `data` is a submission's public JSON; under `--json` it is
/// re-emitted as the same `{submissions:[…], cursor, timed_out}` envelope the long-poll
/// prints (one per event), otherwise as one compact JSON line on stdout (the data
/// channel). Without `--follow` the first submission ends the command (backgrounded
/// ergonomics — fire, get the answer as the result); with `--follow` it keeps printing
/// each as it lands until `timeout`. The cursor comes from each record's own `id`, and
/// only **strictly-forward** ids are accepted (a duplicate/backward/id-less event is
/// ignored) so the client keeps the same no-redeliver contract as the store.
pub(super) async fn consume_submission_stream(
    resp: reqwest::Response,
    since: u64,
    timeout: u64,
    follow: bool,
    json: bool,
) -> ! {
    use tokio_stream::StreamExt as _;
    let status = resp.status();
    if !status.is_success() {
        let payload: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
        let msg = payload
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("the server rejected the stream");
        let code = payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_str())
            .unwrap_or("stream_rejected");
        let exit = if status.is_client_error() { 1 } else { 2 };
        exit_error(
            json,
            exit,
            code,
            &format!("{msg} (HTTP {})", status.as_u16()),
            None,
            None,
        );
    }
    // A 2xx that is not an SSE stream (a proxy / login page returning 200 text/html)
    // must be a clear protocol error, not a silent "timed out with no submission".
    let is_event_stream = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| {
            ct.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("text/event-stream")
        })
        .unwrap_or(false);
    if !is_event_stream {
        exit_error(
            json,
            2,
            "unexpected_content_type",
            "the server did not return an SSE stream (Content-Type text/event-stream)",
            None,
            None,
        );
    }

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout);
    let mut body = resp.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut cursor = since;
    let mut received = 0usize;

    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        let chunk = match tokio::time::timeout(deadline - now, body.next()).await {
            Err(_) => break,      // our logical --timeout elapsed
            Ok(None) => break,    // the server closed the stream
            Ok(Some(Ok(b))) => b, // more stream bytes
            Ok(Some(Err(e))) => {
                // A mid-hold transport error: if we already delivered something this is
                // a normal end; otherwise it is a genuine failure.
                if received > 0 {
                    break;
                }
                exit_error(
                    json,
                    2,
                    "stream_failed",
                    &format!("stream error: {e}"),
                    None,
                    None,
                );
            }
        };
        // Decode over raw bytes (never per-chunk lossy UTF-8) with bounded buffers.
        let mut items = Vec::new();
        if decoder.feed(&chunk, &mut items).is_err() {
            exit_error(
                json,
                2,
                "stream_too_large",
                "the SSE stream exceeded the per-line / per-event size bound",
                None,
                None,
            );
        }
        for SseItem::Submission { id, value } in items {
            // Cursor invariant: accept only a strictly-forward id (skip a duplicate,
            // out-of-order, or id-less/degraded event without counting or printing it).
            if id <= cursor {
                continue;
            }
            cursor = id;
            received += 1;
            if json {
                emit_json_line(&json!({
                    "schema_version": SCHEMA_VERSION,
                    "timed_out": false,
                    "submissions": [value],
                    "cursor": cursor,
                    "warnings": [],
                }));
            } else {
                println!("{}", serde_json::to_string(&value).unwrap_or_default());
            }
            if !follow {
                eprintln!("received 1 submission via stream (next cursor {cursor})");
                std::process::exit(0);
            }
        }
    }

    // The stream ended (timeout or server close). Any delivered submissions → success.
    if received > 0 {
        eprintln!("streamed {received} submission(s) (next cursor {cursor})");
        std::process::exit(0);
    }
    if json {
        emit_json_line(&json!({
            "schema_version": SCHEMA_VERSION,
            "timed_out": true,
            "submissions": [],
            "cursor": cursor,
            "warnings": [],
        }));
    } else {
        eprintln!("no submission before the {timeout}s timeout (re-arm from cursor {cursor})");
    }
    std::process::exit(3);
}

/// Upper bound on one buffered SSE line before the peer is treated as hostile: a
/// submission's public JSON (one `data:` line) plus SSE/envelope slack.
pub(super) const MAX_SSE_LINE_BYTES: usize = crate::submissions::MAX_SUBMISSION_BYTES + 16 * 1024;
/// Upper bound on one event's accumulated `data` across its `data:` lines.
pub(super) const MAX_SSE_EVENT_BYTES: usize = crate::submissions::MAX_SUBMISSION_BYTES + 32 * 1024;

/// One decoded item the SSE stream produced.
pub(super) enum SseItem {
    /// A complete `submission` event whose `data` parsed to an object with a numeric id.
    Submission { id: u64, value: serde_json::Value },
}

/// The peer violated an SSE size bound (a line or event exceeded its cap).
#[derive(Debug)]
pub(super) struct SseOverflow;

/// A bounded, incremental Server-Sent-Events line decoder. It operates on **raw bytes**
/// so a multi-byte UTF-8 code point split across network chunks is never corrupted (a
/// complete line is UTF-8-decoded once, as a whole — the store's payloads are valid
/// UTF-8). Line and event buffers are size-capped, so a hostile or broken `--server`
/// that streams without a newline, or an oversize event, fails closed with
/// [`SseOverflow`] instead of growing memory without bound. Only `submission` events
/// whose `data` is a JSON object with a numeric `id` are surfaced; `id:`/`retry:`/
/// unknown fields and comment (`:`) keep-alives are ignored (the cursor is the record's
/// own `id`).
#[derive(Default)]
pub(super) struct SseDecoder {
    /// Bytes of the current (still-incomplete) line.
    line: Vec<u8>,
    /// The in-progress event's `event:` name (last one wins before dispatch).
    event: Option<Vec<u8>>,
    /// The in-progress event's accumulated `data:` bytes.
    data: Vec<u8>,
}

impl SseDecoder {
    /// Feed a chunk of raw bytes; append any completed `submission` events to `out`.
    pub(super) fn feed(&mut self, chunk: &[u8], out: &mut Vec<SseItem>) -> Result<(), SseOverflow> {
        for &b in chunk {
            if b == b'\n' {
                let mut line = std::mem::take(&mut self.line);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                self.dispatch_line(&line, out)?;
            } else {
                if self.line.len() >= MAX_SSE_LINE_BYTES {
                    return Err(SseOverflow);
                }
                self.line.push(b);
            }
        }
        Ok(())
    }

    /// Process one complete line (SSE field grammar).
    fn dispatch_line(&mut self, line: &[u8], out: &mut Vec<SseItem>) -> Result<(), SseOverflow> {
        if line.is_empty() {
            // Blank line → dispatch the accumulated event, then reset for the next one.
            let is_submission = self.event.as_deref() == Some(b"submission");
            let data = std::mem::take(&mut self.data);
            self.event = None;
            if is_submission
                && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&data)
                && let Some(id) = v.get("id").and_then(|i| i.as_u64())
            {
                out.push(SseItem::Submission { id, value: v });
            }
            return Ok(());
        }
        if line.first() == Some(&b':') {
            return Ok(()); // comment / keep-alive
        }
        // `field: value`; one optional space after the colon is stripped. A line with no
        // colon is a field name with an empty value (per the SSE grammar).
        let (field, mut value) = match line.iter().position(|&b| b == b':') {
            Some(i) => (&line[..i], &line[i + 1..]),
            None => (line, &b""[..]),
        };
        if value.first() == Some(&b' ') {
            value = &value[1..];
        }
        match field {
            b"event" => self.event = Some(value.to_vec()),
            b"data" => {
                // +1 for the joining '\n' between multiple data lines.
                if self.data.len() + value.len() + 1 > MAX_SSE_EVENT_BYTES {
                    return Err(SseOverflow);
                }
                if !self.data.is_empty() {
                    self.data.push(b'\n');
                }
                self.data.extend_from_slice(value);
            }
            // `id:`/`retry:`/unknown fields are ignored — the cursor is the record's id.
            _ => {}
        }
        Ok(())
    }
}
