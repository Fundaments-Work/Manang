//! Append-only JSONL transcripts and audit logs.
//!
//! One JSON value per line. Raw provider blocks are written verbatim; only
//! insignificant whitespace is dropped when a block would otherwise contain a
//! newline (which would break the line framing). String contents are never
//! touched.

use crate::block::Message;
use crate::error::CoreError;
use crate::event::Usage;
use crate::request::RiskClass;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;

/// Removes JSON whitespace outside strings. String bytes are copied as-is.
pub fn compact_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    for c in text.chars() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if !matches!(c, ' ' | '\t' | '\n' | '\r') {
            out.push(c);
        }
    }
    out
}

/// Encodes one entry as a single line without the trailing newline.
pub fn encode_line<T: Serialize>(entry: &T) -> Result<String, CoreError> {
    let line = serde_json::to_string(entry)?;
    if line.contains(['\n', '\r']) {
        Ok(compact_json(&line))
    } else {
        Ok(line)
    }
}

/// Appends one entry to a sink as a line.
pub fn append<W: Write, T: Serialize>(sink: &mut W, entry: &T) -> Result<(), CoreError> {
    let mut line = encode_line(entry)?;
    line.push('\n');
    sink.write_all(line.as_bytes())?;
    Ok(())
}

/// The outcome of reading a log.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadLog<T> {
    pub entries: Vec<T>,
    /// The last line had no newline and did not parse: an interrupted append.
    /// It is skipped, everything before it is intact.
    pub truncated_tail: bool,
}

/// Reads a JSONL log. A bad *complete* line is an error; a bad unterminated
/// final line is reported as a truncated tail.
pub fn read_all<T: DeserializeOwned>(text: &str) -> Result<ReadLog<T>, CoreError> {
    let mut entries = Vec::new();
    let mut truncated_tail = false;
    let terminated = text.ends_with('\n');
    let lines: Vec<&str> = text.split('\n').collect();
    let last_index = lines.len().saturating_sub(1);
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(line) {
            Ok(entry) => entries.push(entry),
            Err(_) if i == last_index && !terminated => truncated_tail = true,
            Err(e) => {
                return Err(CoreError::BadLine {
                    line: i + 1,
                    message: e.to_string(),
                });
            }
        }
    }
    Ok(ReadLog {
        entries,
        truncated_tail,
    })
}

/// One transcript line: a message with its position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub seq: u64,
    pub message: Message,
}

/// Lowercase hex SHA-256 of a payload.
pub fn payload_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        out.push(char::from(b"0123456789abcdef"[usize::from(byte & 15)]));
    }
    out
}

/// What happened, for the audit log. Never contains an API key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEvent {
    /// A request left the machine.
    Request {
        provider: String,
        model: String,
        request_id: Option<String>,
        payload_sha256: String,
        payload_bytes: u64,
        redactions: u32,
        /// The exact payload sent, when the policy keeps it.
        payload: Option<String>,
    },
    ToolCall {
        tool: String,
        risk: RiskClass,
        arguments_sha256: String,
    },
    Approval {
        tool: String,
        approved: bool,
        /// `auto`, `user` or `policy`.
        by: String,
    },
    Usage {
        model: String,
        usage: Usage,
        cost_micro_usd: Option<u64>,
    },
    Note {
        text: String,
    },
}

/// One audit line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Milliseconds since the Unix epoch, supplied by the caller (this crate
    /// has no clock).
    pub ts_ms: u64,
    pub session: String,
    pub event: AuditEvent,
}
