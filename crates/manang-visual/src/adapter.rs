//! The native format-adapter interface (the Rust shape of `wit/format.wit`).
//!
//! Adapters work in **UTF-8 byte offsets**; the session host converts to the
//! UTF-16 units of the editor. An adapter never writes the buffer: commands
//! return an [`EditPlan`] and the web view applies it as one transaction.

use std::ops::Range;

use serde::Serialize;

use crate::error::AdapterError;

/// One syntactic construct, as an adapter reports it. The host clamps,
/// sorts and nests these into the frozen table layout, so an adapter may
/// return them in any order and need not compute layers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSpan {
    pub range: Range<usize>,
    pub kind: u16,
    pub aux: u8,
    pub attr: Option<String>,
}

impl RawSpan {
    pub fn new(range: Range<usize>, kind: u16) -> Self {
        Self {
            range,
            kind,
            aux: 0,
            attr: None,
        }
    }

    #[must_use]
    pub fn aux(mut self, aux: u8) -> Self {
        self.aux = aux;
        self
    }

    #[must_use]
    pub fn attr(mut self, attr: impl Into<String>) -> Self {
        self.attr = Some(attr.into());
        self
    }
}

/// One text edit in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: Range<usize>,
    pub replacement: String,
}

impl TextEdit {
    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Self {
            range: at..at,
            replacement: text.into(),
        }
    }

    pub fn delete(range: Range<usize>) -> Self {
        Self {
            range,
            replacement: String::new(),
        }
    }

    pub fn replace(range: Range<usize>, text: impl Into<String>) -> Self {
        Self {
            range,
            replacement: text.into(),
        }
    }
}

/// What a command wants done: edits sorted ascending, non-overlapping and all
/// in the coordinates of the text *before* them, plus where the selection
/// should end up in the text *after* them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditPlan {
    pub edits: Vec<TextEdit>,
    pub selection: Option<Range<usize>>,
}

/// One heading of the document outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    pub level: u8,
    pub title: String,
    pub range: Range<usize>,
}

/// Toolbar state of one command at a selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandState {
    pub command: String,
    pub enabled: bool,
    pub active: bool,
}

impl CommandState {
    pub fn new(command: &str, enabled: bool, active: bool) -> Self {
        Self {
            command: command.to_owned(),
            enabled,
            active,
        }
    }
}

/// What an adapter declares (the manifest's `format-adapters` entry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdapterInfo {
    pub id: String,
    pub title: String,
    /// Lowercase, no dot.
    pub extensions: Vec<String>,
    /// Manifest names of the kinds it produces.
    pub kinds: Vec<String>,
    /// Commands it implements.
    pub commands: Vec<String>,
}

/// A parsed document, kept alive between calls (ADR 0003 addendum).
pub trait AdapterSession: Send {
    /// Replace `range` (bytes of the current text) with `text`, reparsing
    /// incrementally. Returns the range of the *new* text whose spans may have
    /// changed. Called once per change, last change of a batch first.
    fn edit(&mut self, range: Range<usize>, text: &str) -> Result<Range<usize>, AdapterError>;

    /// Every span intersecting `viewport`, whole, in any order.
    fn spans(&self, viewport: Range<usize>, out: &mut Vec<RawSpan>) -> Result<(), AdapterError>;

    /// Headings in document order.
    fn outline(&self) -> Vec<OutlineItem>;

    /// Enabled and active state of every command at `selection`.
    fn command_state(&self, selection: Range<usize>) -> Vec<CommandState>;

    /// Plan the edits of `command`. `args` is the command's single string
    /// argument (empty when it takes none).
    fn run_command(
        &self,
        command: &str,
        selection: Range<usize>,
        args: &str,
    ) -> Result<EditPlan, AdapterError>;
}

/// A format: declares itself and opens sessions.
pub trait FormatAdapter: Send + Sync {
    fn info(&self) -> &AdapterInfo;

    /// Parse `text` into a new session.
    fn open(&self, text: &str) -> Result<Box<dyn AdapterSession>, AdapterError>;
}
