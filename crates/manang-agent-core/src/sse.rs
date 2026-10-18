//! A Server-Sent Events parser (WHATWG "Server-sent events" parsing rules).
//!
//! Push bytes in as they arrive, in chunks of any size split anywhere; get
//! complete events out. Handles LF, CRLF and bare CR line ends (also when a
//! CRLF is split across chunks), comment lines (pings), multi-line `data`,
//! and a leading byte-order mark. Input is bounded so a hostile server cannot
//! grow memory without limit.

use crate::error::CoreError;

/// Largest accepted size of one event (all its lines together), in bytes.
pub const MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;

/// One dispatched event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` field, if any.
    pub event: Option<String>,
    /// The `data:` lines joined with `\n`.
    pub data: String,
    /// The last `id:` seen.
    pub id: Option<String>,
}

/// Incremental parser. `Default` gives the standard limit.
#[derive(Debug, Default)]
pub struct SseParser {
    line: Vec<u8>,
    event: Option<String>,
    data: String,
    has_data: bool,
    id: Option<String>,
    skip_lf: bool,
    started: bool,
    event_bytes: usize,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds a chunk and returns the events completed by it.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, CoreError> {
        let mut events = Vec::new();
        for &byte in chunk {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\n' | b'\r' => {
                    self.skip_lf = byte == b'\r';
                    if let Some(event) = self.end_line() {
                        events.push(event);
                    }
                }
                _ => {
                    self.line.push(byte);
                    self.event_bytes += 1;
                    if self.event_bytes > MAX_EVENT_BYTES {
                        return Err(CoreError::BadLine {
                            line: 0,
                            message: format!("SSE event larger than {MAX_EVENT_BYTES} bytes"),
                        });
                    }
                }
            }
        }
        Ok(events)
    }

    /// Ends the input. An event not terminated by a blank line is dropped,
    /// as the specification requires.
    pub fn finish(self) {}

    fn end_line(&mut self) -> Option<SseEvent> {
        let raw = std::mem::take(&mut self.line);
        let mut text = String::from_utf8_lossy(&raw).into_owned();
        if !self.started {
            self.started = true;
            if let Some(stripped) = text.strip_prefix('\u{feff}') {
                text = stripped.to_owned();
            }
        }
        if text.is_empty() {
            return self.dispatch();
        }
        if text.starts_with(':') {
            return None;
        }
        let (field, value) = match text.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (text.as_str(), ""),
        };
        match field {
            "event" => self.event = Some(value.to_owned()),
            "data" => {
                if self.has_data {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.has_data = true;
            }
            "id" if !value.contains('\0') => self.id = Some(value.to_owned()),
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        self.event_bytes = 0;
        let event = self.event.take();
        if !self.has_data {
            return None;
        }
        self.has_data = false;
        Some(SseEvent {
            event,
            data: std::mem::take(&mut self.data),
            id: self.id.clone(),
        })
    }
}
