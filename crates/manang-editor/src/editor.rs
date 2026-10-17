use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use manang_kernel::BufferService;
use parking_lot::Mutex;
use ropey::Rope;
use serde::Serialize;

use crate::error::EditorError;

/// Stable handle to an open document.
pub type DocId = u64;

struct Document {
    path: Option<PathBuf>,
    rope: Rope,
    revision: u64,
    saved_revision: u64,
}

impl Document {
    fn edit_range(&self, from: usize, to: usize) -> Result<(), EditorError> {
        let len = self.rope.len_chars();
        if from > to || to > len {
            return Err(EditorError::OutOfRange { from, to, len });
        }
        Ok(())
    }
}

/// Snapshot of a document's state for the shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocInfo {
    pub id: DocId,
    pub path: Option<String>,
    pub chars: usize,
    pub lines: usize,
    pub revision: u64,
    pub dirty: bool,
}

#[derive(Default)]
struct State {
    next_id: DocId,
    active: Option<DocId>,
    documents: BTreeMap<DocId, Document>,
}

impl State {
    fn resolve(&self, id: Option<DocId>) -> Result<DocId, EditorError> {
        let id = id.or(self.active).ok_or(EditorError::NoActiveDocument)?;
        if self.documents.contains_key(&id) {
            Ok(id)
        } else {
            Err(EditorError::NoSuchDocument(id))
        }
    }

    fn doc_mut(&mut self, id: Option<DocId>) -> Result<(DocId, &mut Document), EditorError> {
        let id = self.resolve(id)?;
        let doc = self
            .documents
            .get_mut(&id)
            .ok_or(EditorError::NoSuchDocument(id))?;
        Ok((id, doc))
    }

    fn add(&mut self, path: Option<PathBuf>, rope: Rope) -> DocId {
        self.next_id += 1;
        let id = self.next_id;
        self.documents.insert(
            id,
            Document {
                path,
                rope,
                revision: 0,
                saved_revision: 0,
            },
        );
        self.active = Some(id);
        id
    }
}

/// Open documents and the active selection. Cheap to share behind an `Arc`.
#[derive(Default)]
pub struct Editor {
    state: Mutex<State>,
}

impl Editor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty untitled document and make it active.
    pub fn create(&self) -> DocId {
        self.state.lock().add(None, Rope::new())
    }

    /// Load a UTF-8 file as a new active document.
    pub fn open(&self, path: &Path) -> Result<DocId, EditorError> {
        let text = fs::read_to_string(path).map_err(|source| EditorError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Ok(self
            .state
            .lock()
            .add(Some(path.to_owned()), Rope::from_str(&text)))
    }

    /// Write a document to its path, or to `path` (which then becomes its path).
    pub fn save(&self, id: Option<DocId>, path: Option<&Path>) -> Result<(), EditorError> {
        let mut state = self.state.lock();
        let (_, doc) = state.doc_mut(id)?;
        if let Some(path) = path {
            doc.path = Some(path.to_owned());
        }
        let target = doc.path.clone().ok_or(EditorError::NoPath)?;
        // Stream rope chunks: no multi-megabyte intermediate String.
        let mut out = fs::File::create(&target).map_err(|source| EditorError::Io {
            path: target.display().to_string(),
            source,
        })?;
        doc.rope
            .write_to(&mut out)
            .map_err(|source| EditorError::Io {
                path: target.display().to_string(),
                source,
            })?;
        doc.saved_revision = doc.revision;
        Ok(())
    }

    pub fn activate(&self, id: DocId) -> Result<(), EditorError> {
        let mut state = self.state.lock();
        let id = state.resolve(Some(id))?;
        state.active = Some(id);
        Ok(())
    }

    /// Insert `text` at character index `at`. Returns the new revision.
    pub fn insert(&self, id: Option<DocId>, at: usize, text: &str) -> Result<u64, EditorError> {
        let mut state = self.state.lock();
        let (_, doc) = state.doc_mut(id)?;
        doc.edit_range(at, at)?;
        doc.rope.insert(at, text);
        doc.revision += 1;
        Ok(doc.revision)
    }

    /// Delete the character range `from..to`. Returns the new revision.
    pub fn delete(&self, id: Option<DocId>, from: usize, to: usize) -> Result<u64, EditorError> {
        let mut state = self.state.lock();
        let (_, doc) = state.doc_mut(id)?;
        doc.edit_range(from, to)?;
        doc.rope.remove(from..to);
        doc.revision += 1;
        Ok(doc.revision)
    }

    /// The full document text. Allocates; prefer ranged reads for big files.
    pub fn text(&self, id: Option<DocId>) -> Result<String, EditorError> {
        let mut state = self.state.lock();
        let (_, doc) = state.doc_mut(id)?;
        Ok(doc.rope.to_string())
    }

    pub fn info(&self, id: Option<DocId>) -> Result<DocInfo, EditorError> {
        let mut state = self.state.lock();
        let (id, doc) = state.doc_mut(id)?;
        Ok(DocInfo {
            id,
            path: doc.path.as_ref().map(|p| p.display().to_string()),
            chars: doc.rope.len_chars(),
            lines: doc.rope.len_lines(),
            revision: doc.revision,
            dirty: doc.revision != doc.saved_revision,
        })
    }

    /// Every open document, in creation order.
    pub fn list(&self) -> Vec<DocInfo> {
        let ids: Vec<DocId> = self.state.lock().documents.keys().copied().collect();
        ids.into_iter()
            .filter_map(|id| self.info(Some(id)).ok())
            .collect()
    }
}

impl BufferService for Editor {
    fn read_active(&self) -> Result<String, String> {
        self.text(None).map_err(|e| e.to_string())
    }

    fn write_active(&self, text: &str) -> Result<(), String> {
        let mut state = self.state.lock();
        let (_, doc) = state.doc_mut(None).map_err(|e| e.to_string())?;
        doc.rope = Rope::from_str(text);
        doc.revision += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn edits_and_info() {
        let ed = Editor::new();
        let id = ed.create();
        ed.insert(None, 0, "héllo wörld").expect("insert");
        ed.delete(None, 5, 11).expect("delete");
        assert_eq!(ed.text(Some(id)).expect("text"), "héllo");
        let info = ed.info(None).expect("info");
        assert_eq!((info.chars, info.revision, info.dirty), (5, 2, true));
    }

    #[test]
    fn out_of_range_edits_are_errors_not_panics() {
        let ed = Editor::new();
        ed.create();
        ed.insert(None, 0, "abc").expect("insert");
        assert!(matches!(
            ed.insert(None, 4, "x"),
            Err(EditorError::OutOfRange { .. })
        ));
        assert!(matches!(
            ed.delete(None, 2, 1),
            Err(EditorError::OutOfRange { .. })
        ));
        assert!(matches!(
            ed.delete(None, 0, 9),
            Err(EditorError::OutOfRange { .. })
        ));
        assert_eq!(
            ed.text(None).expect("text"),
            "abc",
            "failed edits changed nothing"
        );
    }

    #[test]
    fn no_document_errors() {
        let ed = Editor::new();
        assert!(matches!(ed.text(None), Err(EditorError::NoActiveDocument)));
        ed.create();
        assert!(matches!(
            ed.text(Some(99)),
            Err(EditorError::NoSuchDocument(99))
        ));
        assert!(matches!(ed.save(None, None), Err(EditorError::NoPath)));
    }

    #[test]
    fn open_save_round_trip_and_dirty_tracking() {
        let dir = std::env::temp_dir().join(format!("manang-editor-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("a.typ");
        fs::write(&path, "= Title\n").expect("seed");

        let ed = Editor::new();
        let id = ed.open(&path).expect("open");
        assert!(!ed.info(Some(id)).expect("info").dirty);
        ed.insert(Some(id), 8, "body\n").expect("insert");
        assert!(ed.info(Some(id)).expect("info").dirty);
        ed.save(Some(id), None).expect("save");
        assert!(!ed.info(Some(id)).expect("info").dirty);
        assert_eq!(fs::read_to_string(&path).expect("read"), "= Title\nbody\n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn buffer_service_replaces_active_text() {
        let ed = Editor::new();
        ed.create();
        ed.write_active("new").expect("write");
        assert_eq!(ed.read_active().expect("read"), "new");
        assert_eq!(ed.info(None).expect("info").revision, 1);
    }

    #[derive(Debug, Clone)]
    enum Op {
        Insert(usize, String),
        Delete(usize, usize),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0usize..64, "\\PC{0,8}").prop_map(|(at, s)| Op::Insert(at, s)),
            (0usize..64, 0usize..64).prop_map(|(a, b)| Op::Delete(a, b)),
        ]
    }

    proptest! {
        /// The rope-backed editor must agree with a plain `Vec<char>` oracle and
        /// never panic, including on out-of-range edits.
        #[test]
        fn matches_a_vec_char_oracle(ops in proptest::collection::vec(op(), 1..100)) {
            let ed = Editor::new();
            ed.create();
            let mut oracle: Vec<char> = Vec::new();
            for op in ops {
                match op {
                    Op::Insert(at, s) => {
                        let ok = ed.insert(None, at, &s).is_ok();
                        prop_assert_eq!(ok, at <= oracle.len());
                        if ok {
                            oracle.splice(at..at, s.chars());
                        }
                    }
                    Op::Delete(a, b) => {
                        let ok = ed.delete(None, a, b).is_ok();
                        prop_assert_eq!(ok, a <= b && b <= oracle.len());
                        if ok {
                            oracle.drain(a..b);
                        }
                    }
                }
                prop_assert_eq!(ed.text(None).expect("text"), oracle.iter().collect::<String>());
            }
        }
    }
}
