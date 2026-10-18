//! One open document: a rope (the UTF-16 <-> byte converter) plus the
//! adapter's incremental parse state, and the host that owns many of them.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use ropey::Rope;
use serde::{Deserialize, Serialize};

use crate::adapter::{
    AdapterInfo, AdapterSession, CommandState, EditPlan, FormatAdapter, OutlineItem, RawSpan,
};
use crate::error::VisualError;
use crate::registry::AdapterRegistry;
use crate::table::SpanTable;

/// Most sessions one host keeps.
pub const MAX_SESSIONS: usize = 64;
/// Largest document a session accepts, in bytes.
pub const MAX_DOC_BYTES: usize = 64 * 1024 * 1024;

/// One change in UTF-16 units: `{from, to, insert}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub from: usize,
    pub to: usize,
    #[serde(default)]
    pub insert: String,
}

/// What `edit` reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditOutcome {
    pub revision: u64,
    pub len: usize,
    /// Region of the new document whose spans may have changed.
    pub dirty: Option<DirtyRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DirtyRange {
    pub from: usize,
    pub to: usize,
}

/// A command's plan in UTF-16 units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanOut {
    pub changes: Vec<ChangeOut>,
    pub selection: Option<DirtyRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeOut {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

/// A heading in UTF-16 units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutlineOut {
    pub level: u8,
    pub title: String,
    pub from: usize,
    pub to: usize,
}

/// A single open document.
pub struct Session {
    adapter: Arc<dyn FormatAdapter>,
    inner: Box<dyn AdapterSession>,
    rope: Rope,
    revision: u64,
}

impl Session {
    pub fn new(adapter: Arc<dyn FormatAdapter>, text: &str) -> Result<Self, VisualError> {
        if text.len() > MAX_DOC_BYTES {
            return Err(VisualError::TooLarge(format!(
                "the document is {} bytes; visual mode handles up to {MAX_DOC_BYTES}",
                text.len()
            )));
        }
        let inner = adapter.open(text)?;
        Ok(Self {
            adapter,
            inner,
            rope: Rope::from_str(text),
            revision: 0,
        })
    }

    pub fn adapter_id(&self) -> &str {
        &self.adapter.info().id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Document length in UTF-16 units.
    pub fn len(&self) -> usize {
        self.rope.len_utf16_cu()
    }

    pub fn is_empty(&self) -> bool {
        self.rope.len_bytes() == 0
    }

    /// Replace the whole text (resync). The revision keeps growing.
    pub fn reset(&mut self, text: &str) -> Result<(), VisualError> {
        if text.len() > MAX_DOC_BYTES {
            return Err(VisualError::TooLarge(format!(
                "the document is {} bytes; visual mode handles up to {MAX_DOC_BYTES}",
                text.len()
            )));
        }
        self.inner = self.adapter.open(text)?;
        self.rope = Rope::from_str(text);
        self.revision += 1;
        Ok(())
    }

    fn byte_of(&self, utf16: usize) -> usize {
        let u = utf16.min(self.rope.len_utf16_cu());
        self.rope.char_to_byte(self.rope.utf16_cu_to_char(u))
    }

    fn utf16_of(&self, byte: usize) -> usize {
        let b = byte.min(self.rope.len_bytes());
        self.rope.char_to_utf16_cu(self.rope.byte_to_char(b))
    }

    /// Apply a batch of changes: ascending, non-overlapping, in the document
    /// before the batch. `expect_len` is the length after the batch; a
    /// mismatch means the copies diverged and nothing is applied.
    pub fn edit(
        &mut self,
        changes: &[Change],
        expect_len: usize,
    ) -> Result<EditOutcome, VisualError> {
        let before = self.len();
        let mut prev_end = 0usize;
        let mut delta: isize = 0;
        for c in changes {
            if c.from > c.to || c.to > before || c.from < prev_end {
                return Err(VisualError::BadArgs(format!(
                    "change {}..{} is unordered or outside the document ({before} units)",
                    c.from, c.to
                )));
            }
            prev_end = c.to;
            delta += c.insert.encode_utf16().count() as isize - (c.to - c.from) as isize;
        }
        let after = before as isize + delta;
        if after < 0 || after as usize != expect_len {
            return Err(VisualError::Desync(format!(
                "session has {before} units, the changes make {after}, the editor reports {expect_len}"
            )));
        }
        if self.rope.len_bytes() + changes.iter().map(|c| c.insert.len()).sum::<usize>()
            > MAX_DOC_BYTES
        {
            return Err(VisualError::TooLarge(
                "the document grew too large".to_owned(),
            ));
        }

        // Last change first: earlier positions stay valid while we go.
        let mut dirty: Option<Range<usize>> = None;
        let mut adapter_failed = false;
        for c in changes.iter().rev() {
            let char_from = self.rope.utf16_cu_to_char(c.from);
            let char_to = self.rope.utf16_cu_to_char(c.to);
            let byte_from = self.rope.char_to_byte(char_from);
            let byte_to = self.rope.char_to_byte(char_to);
            self.rope.remove(char_from..char_to);
            self.rope.insert(char_from, &c.insert);
            let shift = c.insert.len() as isize - (byte_to - byte_from) as isize;
            if let Some(d) = &mut dirty {
                d.start = (d.start as isize + shift).max(0) as usize;
                d.end = (d.end as isize + shift).max(0) as usize;
            }
            if adapter_failed {
                continue;
            }
            match self.inner.edit(byte_from..byte_to, &c.insert) {
                Ok(r) => {
                    dirty = Some(match dirty.take() {
                        Some(d) => d.start.min(r.start)..d.end.max(r.end),
                        None => r,
                    });
                }
                Err(_) => adapter_failed = true,
            }
        }
        if adapter_failed {
            // State is derived: rebuild it from the rope.
            let text = self.rope.to_string();
            self.inner = self.adapter.open(&text)?;
            dirty = Some(0..self.rope.len_bytes());
        }
        self.revision += 1;
        let dirty = dirty.map(|d| DirtyRange {
            from: self.utf16_of(d.start),
            to: self.utf16_of(d.end),
        });
        Ok(EditOutcome {
            revision: self.revision,
            len: self.len(),
            dirty,
        })
    }

    /// The span table of the viewport `from..to` (UTF-16 units).
    pub fn blocks(&self, from: usize, to: usize) -> Result<SpanTable, VisualError> {
        let len16 = self.len();
        let from = from.min(len16);
        let to = to.clamp(from, len16);
        let viewport = self.byte_of(from)..self.byte_of(to);
        let mut raw: Vec<RawSpan> = Vec::new();
        self.inner.spans(viewport.clone(), &mut raw)?;
        // Inside an all-ASCII viewport a byte offset is a UTF-16 offset plus a
        // constant, which spares two rope lookups per span (most of the cost
        // of a table). Positions outside it, or in any other text, use the rope.
        let base16 = self.utf16_of(viewport.start);
        let ascii = self
            .rope
            .byte_slice(viewport.clone())
            .chunks()
            .all(str::is_ascii);
        let convert = |b: usize| -> usize {
            if ascii && b >= viewport.start && b <= viewport.end {
                base16 + (b - viewport.start)
            } else {
                self.utf16_of(b)
            }
        };
        for s in &mut raw {
            s.range = convert(s.range.start)..convert(s.range.end);
        }
        Ok(SpanTable::build(raw, len16, from, to, self.revision))
    }

    pub fn outline(&self) -> Vec<OutlineOut> {
        self.inner
            .outline()
            .into_iter()
            .map(|o: OutlineItem| OutlineOut {
                level: o.level,
                title: o.title,
                from: self.utf16_of(o.range.start),
                to: self.utf16_of(o.range.end),
            })
            .collect()
    }

    pub fn command_state(&self, from: usize, to: usize) -> Vec<CommandState> {
        let len16 = self.len();
        let from = from.min(len16);
        let to = to.clamp(from, len16);
        self.inner
            .command_state(self.byte_of(from)..self.byte_of(to))
    }

    /// Plan `command` at the selection `from..to`, in UTF-16 units.
    pub fn run_command(
        &self,
        command: &str,
        from: usize,
        to: usize,
        args: &str,
    ) -> Result<PlanOut, VisualError> {
        let len16 = self.len();
        let from = from.min(len16);
        let to = to.clamp(from, len16);
        let plan: EditPlan =
            self.inner
                .run_command(command, self.byte_of(from)..self.byte_of(to), args)?;
        let bytes = self.rope.len_bytes();
        let mut prev_end = 0usize;
        for e in &plan.edits {
            if e.range.start > e.range.end || e.range.end > bytes || e.range.start < prev_end {
                return Err(VisualError::Failed(format!(
                    "adapter `{}` returned an invalid plan for `{command}`",
                    self.adapter_id()
                )));
            }
            prev_end = e.range.end;
        }
        let changes: Vec<ChangeOut> = plan
            .edits
            .iter()
            .map(|e| ChangeOut {
                from: self.utf16_of(e.range.start),
                to: self.utf16_of(e.range.end),
                insert: e.replacement.clone(),
            })
            .collect();
        let selection = match plan.selection {
            None => None,
            Some(sel) => {
                // Selection is in the text after the edits: apply them to a
                // cheap copy-on-write clone of the rope to convert it.
                let mut after = self.rope.clone();
                for e in plan.edits.iter().rev() {
                    let cf = after.byte_to_char(e.range.start);
                    let ct = after.byte_to_char(e.range.end);
                    after.remove(cf..ct);
                    after.insert(cf, &e.replacement);
                }
                let conv =
                    |b: usize| after.char_to_utf16_cu(after.byte_to_char(b.min(after.len_bytes())));
                Some(DirtyRange {
                    from: conv(sel.start),
                    to: conv(sel.end),
                })
            }
        };
        Ok(PlanOut { changes, selection })
    }

    /// The current text (tests and resync checks; allocates).
    pub fn text(&self) -> String {
        self.rope.to_string()
    }
}

/// What `open` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenInfo {
    pub session: u32,
    pub adapter: String,
    pub revision: u64,
    pub len: usize,
}

/// Many sessions and the registry of adapters that open them. Calls on one
/// session serialise; calls on different sessions run in parallel.
#[derive(Default)]
pub struct SessionHost {
    registry: RwLock<AdapterRegistry>,
    sessions: Mutex<HashMap<u32, Arc<Mutex<Session>>>>,
    next: Mutex<u32>,
}

impl SessionHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an adapter.
    pub fn register(&self, adapter: Arc<dyn FormatAdapter>) -> Result<(), VisualError> {
        self.registry.write().register(adapter)
    }

    pub fn unregister(&self, id: &str) -> bool {
        self.registry.write().unregister(id)
    }

    pub fn adapters(&self) -> Vec<AdapterInfo> {
        self.registry.read().infos()
    }

    /// Resolve the adapter for an open: by id, else by `path` extension.
    pub fn resolve(
        &self,
        adapter: Option<&str>,
        path: Option<&str>,
    ) -> Result<Arc<dyn FormatAdapter>, VisualError> {
        let reg = self.registry.read();
        match (adapter, path) {
            (Some(id), _) => reg
                .get(id)
                .ok_or_else(|| VisualError::NoAdapter(format!("no format adapter `{id}`"))),
            (None, Some(p)) => reg
                .for_path(p)
                .ok_or_else(|| VisualError::NoAdapter(format!("no format adapter for `{p}`"))),
            (None, None) => Err(VisualError::BadArgs(
                "`adapter` or `path` is required".to_owned(),
            )),
        }
    }

    /// Open a session over `text`.
    pub fn open(
        &self,
        text: &str,
        adapter: Option<&str>,
        path: Option<&str>,
    ) -> Result<OpenInfo, VisualError> {
        let adapter = self.resolve(adapter, path)?;
        let session = Session::new(adapter, text)?;
        let mut sessions = self.sessions.lock();
        if sessions.len() >= MAX_SESSIONS {
            return Err(VisualError::TooLarge(format!(
                "at most {MAX_SESSIONS} visual sessions can be open"
            )));
        }
        let id = {
            let mut next = self.next.lock();
            *next += 1;
            *next
        };
        let info = OpenInfo {
            session: id,
            adapter: session.adapter_id().to_owned(),
            revision: 0,
            len: session.len(),
        };
        sessions.insert(id, Arc::new(Mutex::new(session)));
        Ok(info)
    }

    fn get(&self, id: u32) -> Result<Arc<Mutex<Session>>, VisualError> {
        self.sessions
            .lock()
            .get(&id)
            .cloned()
            .ok_or(VisualError::NoSession(id))
    }

    /// Run `f` on a session.
    pub fn with<T>(
        &self,
        id: u32,
        f: impl FnOnce(&mut Session) -> Result<T, VisualError>,
    ) -> Result<T, VisualError> {
        let session = self.get(id)?;
        let mut guard = session.lock();
        f(&mut guard)
    }

    pub fn close(&self, id: u32) -> bool {
        self.sessions.lock().remove(&id).is_some()
    }

    pub fn open_count(&self) -> usize {
        self.sessions.lock().len()
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    //! A toy adapter: every line starting with `# ` is a heading and every
    //! `*word*` is strong. Reparses by scanning, so it is trivially correct,
    //! which makes it the oracle for the host's invariants.

    use super::*;
    use crate::adapter::{AdapterInfo, TextEdit};
    use crate::error::AdapterError;
    use crate::kind;

    pub struct Toy {
        info: AdapterInfo,
    }

    impl Toy {
        pub fn new() -> Self {
            Self {
                info: AdapterInfo {
                    id: "toy".into(),
                    title: "Toy".into(),
                    extensions: vec!["toy".into()],
                    kinds: vec!["heading".into(), "strong".into()],
                    commands: vec!["bold".into()],
                },
            }
        }
    }

    impl FormatAdapter for Toy {
        fn info(&self) -> &AdapterInfo {
            &self.info
        }
        fn open(&self, text: &str) -> Result<Box<dyn AdapterSession>, AdapterError> {
            Ok(Box::new(ToySession {
                text: text.to_owned(),
            }))
        }
    }

    pub struct ToySession {
        pub text: String,
    }

    impl AdapterSession for ToySession {
        fn edit(&mut self, range: Range<usize>, text: &str) -> Result<Range<usize>, AdapterError> {
            self.text.replace_range(range.clone(), text);
            Ok(range.start..range.start + text.len())
        }
        fn spans(&self, _v: Range<usize>, out: &mut Vec<RawSpan>) -> Result<(), AdapterError> {
            let mut at = 0;
            for line in self.text.split_inclusive('\n') {
                if line.starts_with("# ") {
                    out.push(RawSpan::new(at..at + line.trim_end().len(), kind::HEADING).aux(1));
                    out.push(RawSpan::new(at..at + 2, kind::MARKER));
                }
                let b = line.as_bytes();
                let mut i = 0;
                while i < b.len() {
                    if b[i] == b'*'
                        && let Some(j) = line[i + 1..].find('*')
                    {
                        out.push(RawSpan::new(at + i..at + i + j + 2, kind::STRONG));
                        i += j + 2;
                        continue;
                    }
                    i += 1;
                }
                at += line.len();
            }
            Ok(())
        }
        fn outline(&self) -> Vec<OutlineItem> {
            Vec::new()
        }
        fn command_state(&self, _s: Range<usize>) -> Vec<CommandState> {
            vec![CommandState::new("bold", true, false)]
        }
        fn run_command(
            &self,
            command: &str,
            sel: Range<usize>,
            _args: &str,
        ) -> Result<EditPlan, AdapterError> {
            if command != "bold" {
                return Err(AdapterError::Unsupported(command.to_owned()));
            }
            Ok(EditPlan {
                edits: vec![
                    TextEdit::insert(sel.start, "*"),
                    TextEdit::insert(sel.end, "*"),
                ],
                selection: Some(sel.start + 1..sel.end + 1),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::Toy;
    use super::*;
    use proptest::prelude::*;

    fn host(text: &str) -> (SessionHost, u32) {
        let host = SessionHost::new();
        assert!(host.register(Arc::new(Toy::new())).is_ok());
        let id = host.open(text, Some("toy"), None).map(|o| o.session);
        assert!(id.is_ok());
        (host, id.unwrap_or(0))
    }

    #[test]
    fn opens_by_extension_and_rejects_unknown() {
        let host = SessionHost::new();
        assert!(host.register(Arc::new(Toy::new())).is_ok());
        assert!(host.open("x", None, Some("notes.TOY")).is_ok());
        assert_eq!(
            host.open("x", None, Some("a.xyz")).map_err(|e| e.code()),
            Err("visual.no-adapter")
        );
        assert_eq!(
            host.register(Arc::new(Toy::new())).map_err(|e| e.code()),
            Err("visual.bad-args")
        );
    }

    #[test]
    fn utf16_offsets_survive_astral_characters() {
        // "😀" is 2 UTF-16 units and 4 bytes.
        let (host, id) = host("😀 *b*\n# h\n");
        let table = host.with(id, |s| s.blocks(0, 100)).expect("ok");
        let strong = table.spans.iter().find(|s| s.kind == crate::kind::STRONG);
        assert_eq!(strong.map(|s| (s.start, s.end)), Some((3, 6)));
        assert_eq!(table.doc_len, 11);
    }

    #[test]
    fn edit_updates_text_and_reports_dirty() {
        let (host, id) = host("hello\n# t\n");
        let out = host.with(id, |s| {
            s.edit(
                &[Change {
                    from: 0,
                    to: 0,
                    insert: "*x* ".into(),
                }],
                14,
            )
        });
        let out = out.expect("ok");
        assert_eq!(out.revision, 1);
        assert_eq!(out.dirty, Some(DirtyRange { from: 0, to: 4 }));
        let text = host.with(id, |s| Ok(s.text())).unwrap_or_default();
        assert_eq!(text, "*x* hello\n# t\n");
    }

    #[test]
    fn desync_and_bad_changes_apply_nothing() {
        let (host, id) = host("abc");
        let bad_len = host.with(id, |s| {
            s.edit(
                &[Change {
                    from: 0,
                    to: 1,
                    insert: String::new(),
                }],
                3,
            )
        });
        assert_eq!(bad_len.map_err(|e| e.code()), Err("visual.desync"));
        let oob = host.with(id, |s| {
            s.edit(
                &[Change {
                    from: 2,
                    to: 9,
                    insert: String::new(),
                }],
                0,
            )
        });
        assert_eq!(oob.map_err(|e| e.code()), Err("visual.bad-args"));
        let unordered = host.with(id, |s| {
            s.edit(
                &[
                    Change {
                        from: 2,
                        to: 3,
                        insert: String::new(),
                    },
                    Change {
                        from: 0,
                        to: 1,
                        insert: String::new(),
                    },
                ],
                1,
            )
        });
        assert_eq!(unordered.map_err(|e| e.code()), Err("visual.bad-args"));
        assert_eq!(host.with(id, |s| Ok(s.text())).unwrap_or_default(), "abc");
    }

    #[test]
    fn run_command_converts_the_plan_to_utf16() {
        let (host, id) = host("😀 word");
        let plan = host.with(id, |s| s.run_command("bold", 3, 7, ""));
        let plan = plan.expect("ok");
        assert_eq!(
            plan.changes,
            vec![
                ChangeOut {
                    from: 3,
                    to: 3,
                    insert: "*".into()
                },
                ChangeOut {
                    from: 7,
                    to: 7,
                    insert: "*".into()
                },
            ]
        );
        assert_eq!(plan.selection, Some(DirtyRange { from: 4, to: 8 }));
    }

    #[test]
    fn unknown_session_and_limits() {
        let host = SessionHost::new();
        assert_eq!(
            host.with(9, |_| Ok(())).map_err(|e| e.code()),
            Err("visual.no-session")
        );
        assert!(host.register(Arc::new(Toy::new())).is_ok());
        for _ in 0..MAX_SESSIONS {
            assert!(host.open("", Some("toy"), None).is_ok());
        }
        assert_eq!(
            host.open("", Some("toy"), None).map_err(|e| e.code()),
            Err("visual.too-large")
        );
        assert_eq!(host.open_count(), MAX_SESSIONS);
    }

    fn arb_text() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop_oneof![
                Just("a".to_owned()),
                Just("é".to_owned()),
                Just("😀".to_owned()),
                Just("*".to_owned()),
                Just("\n".to_owned()),
                Just("# ".to_owned()),
                Just(" ".to_owned()),
            ],
            0..40,
        )
        .prop_map(|v| v.concat())
    }

    // Random batches of edits, always valid for the current text: the
    // session text equals a plain-string oracle, every table is valid, and
    // the table of an edited session equals one built from scratch.
    proptest! {
        #[test]
        fn stable_under_random_edits(
            init in arb_text(),
            batches in proptest::collection::vec(
                proptest::collection::vec((0usize..1000, 0usize..6, arb_text()), 1..4), 1..12),
        ) {
            let host = SessionHost::new();
            prop_assert!(host.register(Arc::new(Toy::new())).is_ok());
            let id = host.open(&init, Some("toy"), None).map(|o| o.session).unwrap_or(0);
            let mut oracle: Vec<u16> = init.encode_utf16().collect();
            for batch in batches {
                // Build ascending, non-overlapping changes in UTF-16 units.
                let mut changes: Vec<Change> = Vec::new();
                let mut floor = 0usize;
                let len = oracle.len();
                for (pos, del, ins) in batch {
                    let from = (floor + pos % (len - floor + 1)).min(len);
                    let to = (from + del).min(len);
                    // Do not split surrogate pairs: snap to char boundaries.
                    let from = snap(&oracle, from);
                    let to = snap(&oracle, to).max(from);
                    changes.push(Change { from, to, insert: ins });
                    floor = to;
                }
                let mut next = oracle.clone();
                for c in changes.iter().rev() {
                    let ins: Vec<u16> = c.insert.encode_utf16().collect();
                    next.splice(c.from..c.to, ins);
                }
                let out = host.with(id, |s| s.edit(&changes, next.len()));
                prop_assert!(out.is_ok());
                oracle = next;
                let want = String::from_utf16_lossy(&oracle);
                prop_assert_eq!(host.with(id, |s| Ok(s.text())).unwrap_or_default(), want.clone());

                let table = host.with(id, |s| s.blocks(0, usize::MAX)).expect("ok");
                let fresh = {
                    let h2 = SessionHost::new();
                    let _ = h2.register(Arc::new(Toy::new()));
                    let i2 = h2.open(&want, Some("toy"), None).map(|o| o.session).unwrap_or(0);
                    h2.with(i2, |s| s.blocks(0, usize::MAX)).expect("ok")
                };
                prop_assert_eq!(&table.spans, &fresh.spans);
                for (i, s) in table.spans.iter().enumerate() {
                    prop_assert!(s.start < s.end && s.end as usize <= oracle.len());
                    for q in &table.spans[..i] {
                        prop_assert!(q.end <= s.start || s.end <= q.end);
                        if s.layer == q.layer {
                            prop_assert!(q.end <= s.start);
                        }
                    }
                }
            }
        }
    }

    fn snap(units: &[u16], at: usize) -> usize {
        if at > 0 && at < units.len() && (0xDC00..0xE000).contains(&units[at]) {
            at - 1
        } else {
            at
        }
    }
}
