use mail_parser::mailbox::mbox::MessageIterator;
use mail_parser::{Address, MessageParser, MimeHeaders};

use glasspad::data::types::{CellValue, Dataset, Row};

#[derive(Debug)]
pub enum MboxError {
    NoMessages,
    ParseError(String),
}

impl std::fmt::Display for MboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MboxError::NoMessages => write!(f, "No email messages found in mbox/eml data"),
            MboxError::ParseError(msg) => write!(f, "Email parse error: {}", msg),
        }
    }
}

impl std::error::Error for MboxError {}

/// Parse mbox/eml from raw bytes (handles non-UTF-8 email content).
pub fn parse_mbox_bytes(data: &[u8]) -> Result<Dataset, MboxError> {
    parse_mbox_impl(data)
}

/// Parse mbox/eml from a UTF-8 string (convenience for tests).
#[cfg(test)]
fn parse_mbox_str(data: &str) -> Result<Dataset, MboxError> {
    parse_mbox_impl(data.as_bytes())
}

/// Parse mbox data (RFC 4155) or a single EML file into a Dataset.
fn parse_mbox_impl(bytes: &[u8]) -> Result<Dataset, MboxError> {
    // Try mbox format first (uses MessageIterator)
    let mut rows = Dataset::new();
    let parser = MessageParser::default();

    for (count, raw_message) in MessageIterator::new(std::io::Cursor::new(bytes)).enumerate() {
        let raw = raw_message.map_err(|e| MboxError::ParseError(format!("{:?}", e)))?;
        let msg = parser.parse(raw.contents()).ok_or_else(|| {
            MboxError::ParseError(format!("Failed to parse message #{}", count + 1))
        })?;
        rows.push(message_to_row(&msg, count));
    }

    // If no mbox messages, try as single EML
    if rows.is_empty() {
        let msg = parser.parse(bytes).ok_or(MboxError::NoMessages)?;
        // Verify it has at least a subject or from header
        if msg.subject().is_none() && msg.from().is_none() {
            return Err(MboxError::NoMessages);
        }
        rows.push(message_to_row(&msg, 0));
    }

    Ok(rows)
}

/// Convert a parsed email message into a Dataset row.
fn message_to_row(msg: &mail_parser::Message<'_>, idx: usize) -> Row {
    let mut row = Row::new();

    // ID
    let id = msg
        .message_id()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("msg-{:04}", idx + 1));
    row.insert("id".into(), CellValue::String(id));

    // Date
    let date = msg
        .date()
        .map(|d| CellValue::String(d.to_rfc3339()))
        .unwrap_or(CellValue::Null);
    row.insert("date".into(), date);

    // From
    let (from_addr, from_name) = msg
        .from()
        .map(|a| extract_first_address(a))
        .unwrap_or_default();
    row.insert(
        "from".into(),
        if from_addr.is_empty() {
            CellValue::Null
        } else {
            CellValue::String(from_addr)
        },
    );
    row.insert(
        "from_name".into(),
        if from_name.is_empty() {
            CellValue::Null
        } else {
            CellValue::String(from_name)
        },
    );

    // To, CC, BCC — use null for absent fields
    fn str_or_null(s: String) -> CellValue {
        if s.is_empty() {
            CellValue::Null
        } else {
            CellValue::String(s)
        }
    }
    row.insert(
        "to".into(),
        str_or_null(msg.to().map(extract_all_addresses).unwrap_or_default()),
    );
    row.insert(
        "cc".into(),
        str_or_null(msg.cc().map(extract_all_addresses).unwrap_or_default()),
    );
    row.insert(
        "bcc".into(),
        str_or_null(msg.bcc().map(extract_all_addresses).unwrap_or_default()),
    );

    // Subject
    let subject = msg.subject().unwrap_or("(no subject)").to_string();
    row.insert("subject".into(), CellValue::String(subject));

    // Body — check actual content-type of the first HTML part
    // mail-parser auto-converts text→html, so we verify the original is truly HTML
    let has_native_html = msg.html_body.first().is_some_and(|&part_id| {
        msg.part(part_id).is_some_and(|part| {
            part.content_type()
                .is_some_and(|ct| ct.ctype() == "text" && ct.subtype().unwrap_or("") == "html")
        })
    });
    let (body, body_format) = if has_native_html {
        (msg.body_html(0).unwrap_or_default().to_string(), "html")
    } else if let Some(text) = msg.body_text(0) {
        (text.to_string(), "text")
    } else {
        (String::new(), "text")
    };
    row.insert("body".into(), CellValue::String(body));
    row.insert("body_format".into(), CellValue::String(body_format.into()));

    // Attachments
    let attachments: Vec<String> = msg
        .attachments()
        .map(|part| {
            let name = part.attachment_name().unwrap_or("unnamed");
            let size = part.contents().len();
            let size_str = if size >= 1024 * 1024 {
                format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
            } else if size >= 1024 {
                format!("{} KB", size / 1024)
            } else {
                format!("{} B", size)
            };
            format!("{} ({})", name, size_str)
        })
        .collect();
    row.insert(
        "has_attachments".into(),
        CellValue::Bool(!attachments.is_empty()),
    );
    row.insert(
        "attachments".into(),
        CellValue::String(attachments.join(", ")),
    );

    // Thread ID
    let thread_id = msg
        .in_reply_to()
        .as_text_list()
        .and_then(|v| v.first().map(|s| s.to_string()))
        .or_else(|| {
            msg.references()
                .as_text_list()
                .and_then(|v| v.first().map(|s| s.to_string()))
        })
        .unwrap_or_default();
    row.insert(
        "thread_id".into(),
        if thread_id.is_empty() {
            CellValue::Null
        } else {
            CellValue::String(thread_id)
        },
    );

    // Defaults (not available from raw mbox — consumer can enrich)
    row.insert("is_read".into(), CellValue::Bool(false));
    row.insert("is_flagged".into(), CellValue::Bool(false));
    row.insert("tags".into(), CellValue::Null);
    row.insert("folder".into(), CellValue::Null);

    row
}

/// Extract first email address and display name from an Address field.
fn extract_first_address(addr: &Address<'_>) -> (String, String) {
    match addr {
        Address::List(list) => {
            if let Some(a) = list.first() {
                let email = a.address().unwrap_or("").to_string();
                let name = a
                    .name()
                    .unwrap_or_else(|| a.address().unwrap_or(""))
                    .to_string();
                (email, name)
            } else {
                (String::new(), String::new())
            }
        }
        Address::Group(groups) => {
            if let Some(a) = groups.iter().flat_map(|g| &g.addresses).next() {
                let email = a.address().unwrap_or("").to_string();
                let name = a
                    .name()
                    .unwrap_or_else(|| a.address().unwrap_or(""))
                    .to_string();
                (email, name)
            } else {
                (String::new(), String::new())
            }
        }
    }
}

/// Extract all addresses as comma-separated string (email preferred, name as fallback).
fn extract_all_addresses(addr: &Address<'_>) -> String {
    fn addr_str(a: &mail_parser::Addr<'_>) -> Option<String> {
        a.address()
            .map(|s| s.to_string())
            .or_else(|| a.name().map(|s| s.to_string()))
    }
    match addr {
        Address::List(list) => list
            .iter()
            .filter_map(addr_str)
            .collect::<Vec<_>>()
            .join(", "),
        Address::Group(groups) => groups
            .iter()
            .flat_map(|g| &g.addresses)
            .filter_map(addr_str)
            .collect::<Vec<_>>()
            .join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_mbox() {
        let mbox = "From sender@example.com Mon Apr  7 10:00:00 2026\r\n\
From: Alice <alice@example.com>\r\n\
To: bob@example.com\r\n\
Subject: Hello\r\n\
Date: Mon, 7 Apr 2026 10:00:00 +0300\r\n\
Content-Type: text/plain\r\n\
\r\n\
Hi Bob, how are you?\r\n\
\r\n\
From sender2@example.com Mon Apr  7 11:00:00 2026\r\n\
From: Charlie <charlie@example.com>\r\n\
To: bob@example.com\r\n\
Subject: Meeting tomorrow\r\n\
Date: Mon, 7 Apr 2026 11:00:00 +0300\r\n\
Content-Type: text/plain\r\n\
\r\n\
Don't forget the meeting at 2 PM.\r\n";

        let rows = parse_mbox_str(mbox).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["subject"], CellValue::String("Hello".into()));
        assert_eq!(
            rows[0]["from"],
            CellValue::String("alice@example.com".into())
        );
        assert_eq!(rows[0]["from_name"], CellValue::String("Alice".into()));
        assert_eq!(rows[0]["body_format"], CellValue::String("text".into()));
        assert_eq!(
            rows[1]["subject"],
            CellValue::String("Meeting tomorrow".into())
        );
    }

    #[test]
    fn parse_single_eml() {
        let eml = "From: Test <test@example.com>\r\n\
To: user@example.com\r\n\
Subject: EML test\r\n\
Date: Tue, 8 Apr 2026 09:00:00 +0000\r\n\
Content-Type: text/plain\r\n\
\r\n\
This is a single EML file.\r\n";

        let rows = parse_mbox_str(eml).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["subject"], CellValue::String("EML test".into()));
    }

    #[test]
    fn parse_html_body() {
        let mbox = "From sender@example.com Mon Apr  7 10:00:00 2026\r\n\
From: Alice <alice@example.com>\r\n\
To: bob@example.com\r\n\
Subject: HTML message\r\n\
Date: Mon, 7 Apr 2026 10:00:00 +0000\r\n\
Content-Type: text/html\r\n\
\r\n\
<p>Hello <b>Bob</b>!</p>\r\n";

        let rows = parse_mbox_str(mbox).unwrap();
        assert_eq!(rows[0]["body_format"], CellValue::String("html".into()));
    }

    #[test]
    fn empty_input_returns_error() {
        assert!(parse_mbox_str("").is_err());
    }
}
