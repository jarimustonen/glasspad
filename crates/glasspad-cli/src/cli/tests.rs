use super::config_commands::parse_skill_frontmatter;
use super::data::detect_data_format;
use super::host_serve::is_host_wildcard_bind;
use super::publish::publish_config_candidates_from;
use super::runtime::parse_env_port;
use super::submission_commands::{MAX_SSE_LINE_BYTES, SseDecoder, SseItem};
use super::*;

#[test]
fn host_wildcard_bind_classification_canonicalizes_mapped_ipv4() {
    let wildcard = |raw: &str| is_host_wildcard_bind(raw.parse().unwrap());
    assert!(wildcard("0.0.0.0:0"));
    assert!(wildcard("[::]:8080"));
    assert!(wildcard("[::ffff:0.0.0.0]:8080"));
    assert!(!wildcard("127.0.0.1:0"));
    assert!(!wildcard("[::1]:0"));
    assert!(!wildcard("[::ffff:127.0.0.1]:0"));
    assert!(!wildcard("192.0.2.10:8080"));
}

#[test]
fn skill_frontmatter_parser_accepts_lf_and_crlf() {
    let lf = "---\nname: glasspad\ncli_version: 1.2.3\nschema_version: 1\n---\nbody\n";
    let crlf = lf.replace('\n', "\r\n");
    assert_eq!(
        parse_skill_frontmatter(lf).unwrap(),
        parse_skill_frontmatter(&crlf).unwrap()
    );
}

/// Feed `chunks` to a fresh decoder and collect the ids it surfaces.
fn decode_ids(chunks: &[&[u8]]) -> Result<Vec<u64>, ()> {
    let mut dec = SseDecoder::default();
    let mut out = Vec::new();
    for c in chunks {
        dec.feed(c, &mut out).map_err(|_| ())?;
    }
    Ok(out
        .into_iter()
        .map(|SseItem::Submission { id, .. }| id)
        .collect())
}

#[test]
fn sse_decoder_reassembles_a_utf8_char_split_across_chunks() {
    // A submission whose data contains a multi-byte char (€ = 3 bytes) split at an
    // arbitrary byte boundary must decode intact — the old per-chunk lossy decode
    // corrupted it. Frame: `event: submission\ndata: {"id":7,"v":"€"}\n\n`.
    let frame = b"event: submission\ndata: {\"id\":7,\"v\":\"\xe2\x82\xac\"}\n\n";
    // Split mid-way through the € bytes.
    let cut = frame.iter().position(|&b| b == 0xe2).unwrap() + 1;
    let mut dec = SseDecoder::default();
    let mut out = Vec::new();
    dec.feed(&frame[..cut], &mut out).unwrap();
    dec.feed(&frame[cut..], &mut out).unwrap();
    assert_eq!(out.len(), 1);
    let SseItem::Submission { id, value } = &out[0];
    assert_eq!(*id, 7);
    assert_eq!(value["v"], "€", "the split multi-byte char is intact");
}

#[test]
fn sse_decoder_handles_crlf_comments_and_ignores_non_submission() {
    // CRLF endings, keep-alive comment lines, and a non-`submission` event are all
    // tolerated; only the numeric-id submission event is surfaced.
    let ids = decode_ids(&[
        b": keep-alive\r\n",
        b"event: reload\r\ndata: 1\r\n\r\n",
        b"event: submission\r\nid: 4\r\ndata: {\"id\":4}\r\n\r\n",
    ])
    .unwrap();
    assert_eq!(ids, vec![4]);
}

#[test]
fn sse_decoder_skips_a_submission_without_a_numeric_id() {
    // A degraded/id-less `submission` event is NOT surfaced (the client must never
    // count it as a real submission or fail to advance its cursor).
    let ids = decode_ids(&[b"event: submission\ndata: {\"no\":\"id\"}\n\n"]).unwrap();
    assert!(ids.is_empty());
}

#[test]
fn sse_decoder_bounds_an_oversize_line() {
    // A hostile/broken peer streaming a line with no newline past the cap fails
    // closed (Err), never growing memory without bound.
    let big = vec![b'a'; MAX_SSE_LINE_BYTES + 1];
    let mut dec = SseDecoder::default();
    let mut out = Vec::new();
    assert!(dec.feed(&big, &mut out).is_err());
}

#[test]
fn sse_decoder_streams_multiple_events_from_one_chunk() {
    let ids = decode_ids(&[
        b"event: submission\ndata: {\"id\":1}\n\nevent: submission\ndata: {\"id\":2}\n\n",
    ])
    .unwrap();
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn detected_kind_matches_wrap_classifier() {
    // `create` reports the same authoring level the content route acts on.
    assert!(wrap::is_fragment("<h1>hi</h1>"));
    assert!(!wrap::is_fragment("<!doctype html><html></html>"));
    // BOM + whitespace + leading comment before a real doctype → full document.
    assert!(!wrap::is_fragment(
        "\u{feff}  <!-- x -->\n<!DOCTYPE HTML><html>…"
    ));
}

#[test]
fn one_artifact_snapshot_home_and_title() {
    let snap = server::one_artifact_snapshot("report", "<title>Q3</title><h1>x</h1>".into());
    let sp = snap.space("report").unwrap();
    assert_eq!(sp.home.as_deref(), Some(server::SINGLE_SLUG));
    assert_eq!(sp.nav, vec![server::SINGLE_SLUG.to_string()]);
    assert_eq!(sp.artifact(server::SINGLE_SLUG).unwrap().title, "Q3");
}

#[test]
fn one_artifact_title_falls_back_to_space_name() {
    let snap = server::one_artifact_snapshot("myspace", "<p>no title here</p>".into());
    assert_eq!(
        snap.space("myspace")
            .unwrap()
            .artifact(server::SINGLE_SLUG)
            .unwrap()
            .title,
        "myspace"
    );
}

#[test]
fn publish_config_prefers_xdg_then_falls_back_to_platform_dir() {
    let cfg = |p: &str| PathBuf::from(p).join("glasspad").join("config.yaml");

    // $XDG_CONFIG_HOME (absolute) wins; platform dir follows as fallback.
    assert_eq!(
        publish_config_candidates_from(
            Some(PathBuf::from("/xdg")),
            Some(PathBuf::from("/home/u")),
            Some(PathBuf::from("/home/u/Library/Application Support")),
        ),
        vec![cfg("/xdg"), cfg("/home/u/Library/Application Support")],
    );

    // No XDG → ~/.config first, then the platform dir as backward-compat fallback.
    assert_eq!(
        publish_config_candidates_from(
            None,
            Some(PathBuf::from("/home/u")),
            Some(PathBuf::from("/home/u/Library/Application Support")),
        ),
        vec![
            cfg("/home/u/.config"),
            cfg("/home/u/Library/Application Support")
        ],
    );

    // A relative XDG value is ignored (falls through to ~/.config). On Unix
    // `dirs::config_dir()` echoes the same relative value; it must NOT become a
    // CWD-relative candidate — only the absolute ~/.config path survives.
    assert_eq!(
        publish_config_candidates_from(
            Some(PathBuf::from("relative/dir")),
            Some(PathBuf::from("/home/u")),
            Some(PathBuf::from("relative/dir")),
        ),
        vec![cfg("/home/u/.config")],
    );

    // On Linux the XDG path and platform dir coincide → no duplicate candidate.
    assert_eq!(
        publish_config_candidates_from(
            None,
            Some(PathBuf::from("/home/u")),
            Some(PathBuf::from("/home/u/.config")),
        ),
        vec![cfg("/home/u/.config")],
    );
}

#[test]
fn data_format_inferred_from_extension() {
    assert_eq!(detect_data_format(Path::new("x.csv")), Some("csv"));
    assert_eq!(detect_data_format(Path::new("x.JSON")), Some("json")); // case-insensitive
    assert_eq!(detect_data_format(Path::new("mail.mbox")), Some("mbox"));
    assert_eq!(detect_data_format(Path::new("one.eml")), Some("mbox"));
    assert_eq!(detect_data_format(Path::new("notes.txt")), None);
    assert_eq!(detect_data_format(Path::new("noext")), None);
}

#[test]
fn env_port_parsing_is_strict() {
    // Valid values pass through.
    assert_eq!(parse_env_port("8080"), Ok(8080));
    assert_eq!(parse_env_port("1"), Ok(1));
    assert_eq!(parse_env_port("65535"), Ok(65535));
    assert_eq!(parse_env_port("  3000\n"), Ok(3000)); // surrounding whitespace trimmed
    // Invalid values are rejected with an informative, value-naming message —
    // never coerced or silently defaulted (AI-first §1).
    for bad in [
        "", "   ", "0", "65536", "99999", "-1", "80abc", "abc", "3.14",
    ] {
        let err = parse_env_port(bad).unwrap_err();
        assert!(
            err.contains(PORT_ENV),
            "error for {bad:?} should name the env var: {err}"
        );
    }
    // Out-of-range vs. malformed get distinct diagnostics (AI-first §4).
    assert!(
        parse_env_port("65536")
            .unwrap_err()
            .contains("out of range")
    );
    assert!(
        parse_env_port("99999")
            .unwrap_err()
            .contains("out of range")
    );
    assert!(parse_env_port("0").unwrap_err().contains("out of range"));
    assert!(
        parse_env_port("abc")
            .unwrap_err()
            .contains("not a valid port")
    );
}

#[test]
fn resolve_port_flag_wins_env_independent() {
    // An explicit flag is returned verbatim without consulting the environment,
    // so `--port` always beats `$GLASSPAD_PORT` (AI-first §8). The env→default
    // fallback is covered by `env_port_parsing_is_strict` (the pure parser); we
    // deliberately do not mutate the process environment in tests (unsafe +
    // racy under the parallel test harness).
    assert_eq!(resolve_port(Some(4100), false), 4100);
    assert_eq!(resolve_port(Some(1), true), 1);
}

#[test]
fn cli_and_router_share_one_space_grammar() {
    // The names `open`/`create` accept are exactly what the router serves.
    assert!(artifact_host::valid_space("sales-q3"));
    assert!(!artifact_host::valid_space("api")); // reserved
    assert!(!artifact_host::valid_space("Bad_Name")); // grammar
    assert!(!artifact_host::valid_space("")); // empty
}
