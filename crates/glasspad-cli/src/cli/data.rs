use super::author::read_capped;
use super::runtime::*;
use super::*;

// --- data (legacy-format helper) ------------------------------------------

/// `glasspad data <file> [--format] [--meta]` — parse a legacy CSV/JSON/mbox
/// file into JSON rows on stdout. A standalone convenience over the old data
/// parsers (`glasspad::data`): the section-DSL server that once ingested these
/// formats is gone (Wave 5 / Phase 6), but the parsers remain useful for turning
/// such a file into rows a hand-authored HTML artifact can embed. Never starts a
/// server.
///
/// Output contract (AI-first §10): stdout is the data channel. Under `--json`, a
/// versioned envelope `{schema_version, format, path, row_count, rows[, meta]}`;
/// otherwise the bare rows array (pretty JSON) on stdout with a one-line human
/// summary on stderr. Errors go to stderr via [`exit_error`] with a stable `code`.
pub fn data(file: PathBuf, format: Option<String>, meta: bool, json: bool) {
    use glasspad::data::{infer, limits, types::Dataset};

    // Resolve the format: an explicit `--format` wins, else infer from extension.
    let fmt = match format.as_deref() {
        Some(f) => f.to_string(),
        None => match detect_data_format(&file) {
            Some(f) => f.to_string(),
            None => exit_error(
                json,
                1,
                "unknown_format",
                &format!(
                    "cannot infer a data format from {}: pass --format csv|json|mbox",
                    file.display()
                ),
                Some(&file.display().to_string()),
                Some(vec!["csv".into(), "json".into(), "mbox".into()]),
            ),
        },
    };

    // Read the file, bounded to a 50 MB safety cap. The parsers do not bound by
    // byte count — csv/json/mbox each cap by rows and columns — so this read cap
    // is the only byte bound; the errors below carry the parser's own message.
    let bytes = read_data_file(&file, json);
    // Errors carry a stable `(code, message)` so a UTF-8 failure keeps its own
    // `not_utf8` code instead of collapsing into the generic `parse_failed`.
    let parsed: Result<Dataset, (&'static str, String)> = match fmt.as_str() {
        "csv" => glasspad::data::csv::parse_csv_bytes(&bytes, limits::MAX_CSV_BYTES)
            .map_err(|e| ("parse_failed", e.to_string())),
        "mbox" => glasspad::data::mbox::parse_mbox_bytes(&bytes)
            .map_err(|e| ("parse_failed", e.to_string())),
        "json" => match std::str::from_utf8(&bytes) {
            Ok(s) => {
                glasspad::data::json::parse_json_str(s).map_err(|e| ("parse_failed", e.to_string()))
            }
            Err(_) => Err((
                "not_utf8",
                format!("{} is not valid UTF-8 (JSON must be UTF-8)", file.display()),
            )),
        },
        // `--format` is a fixed enum and `detect_data_format` only yields these
        // three, so any other value here is a programming error, not user input.
        other => unreachable!("format resolved to csv|json|mbox, got {other:?}"),
    };
    let rows = match parsed {
        Ok(r) => r,
        Err((code, msg)) => exit_error(json, 1, code, &msg, None, None),
    };

    let meta_val = if meta {
        Some(infer::infer_dataset_meta(&rows))
    } else {
        None
    };

    if json {
        // `warnings: []` matches the serve/create/open envelopes so a consumer can
        // read the field unconditionally across commands.
        let mut payload = json!({
            "schema_version": SCHEMA_VERSION,
            "format": fmt,
            "path": file.display().to_string(),
            "row_count": rows.len(),
            "rows": rows,
            "warnings": [],
        });
        if let Some(m) = &meta_val {
            // The envelope is a JSON object literal above, so this always succeeds.
            if let Some(obj) = payload.as_object_mut() {
                obj.insert(
                    "meta".into(),
                    serde_json::to_value(m).unwrap_or(serde_json::Value::Null),
                );
            }
        }
        emit_json_line(&payload);
    } else {
        // Bare rows on stdout (composable); human summary + optional meta on stderr.
        // A serialization failure is a system error, not empty/`[]` output — never
        // pass off a truncated array as the real data.
        let out = match serde_json::to_string_pretty(&rows) {
            Ok(s) => s,
            Err(e) => exit_error(
                json,
                2,
                "serialization_failed",
                &format!("cannot serialize parsed rows: {e}"),
                None,
                None,
            ),
        };
        println!("{out}");
        eprintln!(
            "parsed {} row{} from {} ({fmt})",
            rows.len(),
            if rows.len() == 1 { "" } else { "s" },
            file.display()
        );
        if let Some(m) = &meta_val {
            let fields = m
                .fields
                .iter()
                .map(|(k, v)| format!("{k}:{v:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!("fields: {fields}");
        }
    }
}

/// Infer the data format from a file extension: `.csv` → csv, `.json` → json,
/// `.mbox`/`.eml` → mbox. Returns `None` for anything else, so the caller can ask
/// for an explicit `--format`.
pub(super) fn detect_data_format(file: &Path) -> Option<&'static str> {
    match file
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase()
        .as_str()
    {
        "csv" => Some("csv"),
        "json" => Some("json"),
        "mbox" | "eml" => Some("mbox"),
        _ => None,
    }
}

/// Read a data file into memory, bounded to a 50 MB safety cap. Strict like
/// `create`: a missing path, a directory, a non-regular file, or an oversize
/// file each exits with an informative envelope rather than a silent truncation.
pub(super) fn read_data_file(file: &Path, json: bool) -> Vec<u8> {
    let meta = match std::fs::metadata(file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => exit_error(
            json,
            1,
            "no_such_path",
            &format!("no such file: {}", file.display()),
            Some(&file.display().to_string()),
            None,
        ),
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    };
    if meta.is_dir() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is a directory; `data` takes a single file",
                file.display()
            ),
            Some(&file.display().to_string()),
            None,
        );
    }
    // Reject FIFOs / sockets / devices: like `create`, a named pipe reports a
    // zero length (passing the size check) but would then block `open`/read
    // forever. Only a regular file is servable.
    if !meta.is_file() {
        exit_error(
            json,
            1,
            "not_a_file",
            &format!(
                "{} is not a regular file (FIFOs, sockets, and devices are not supported)",
                file.display()
            ),
            Some(&file.display().to_string()),
            None,
        );
    }
    let cap = glasspad::data::limits::MAX_CSV_BYTES as u64;
    if meta.len() > cap {
        exit_error(
            json,
            1,
            "file_too_large",
            &format!(
                "{} is {} bytes, over the {cap}-byte limit",
                file.display(),
                meta.len()
            ),
            None,
            None,
        );
    }
    match read_capped(file, cap) {
        Ok(b) if b.len() as u64 > cap => exit_error(
            json,
            1,
            "file_too_large",
            &format!("{} exceeds the {cap}-byte limit", file.display()),
            None,
            None,
        ),
        Ok(b) => b,
        Err(e) => exit_error(
            json,
            2,
            "io_error",
            &format!("cannot read {}: {e}", file.display()),
            None,
            None,
        ),
    }
}
