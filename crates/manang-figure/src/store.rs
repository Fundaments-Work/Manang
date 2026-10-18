//! Open figures: documents with revisions and delta-based undo/redo.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::FigureError;
use crate::geometry::hit_test;
use crate::ops::{self, Delta, FigureOp};
use crate::scene::{FigureNode, FigureScene, Vec2};
use crate::svg::{SvgExportOptions, render_svg_with};

/// Undo steps kept per figure.
pub const DEFAULT_MAX_HISTORY: usize = 100;
/// Default page width.
pub const DEFAULT_SCENE_WIDTH: f64 = 800.0;
/// Default page height.
pub const DEFAULT_SCENE_HEIGHT: f64 = 600.0;

/// Identifier of an open figure.
pub type FigureId = u64;

/// What applying an operation did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyResult {
    /// Revision after the operation.
    pub revision: u64,
    /// Whether anything changed.
    pub applied: bool,
    /// Whether an undo step is available.
    #[serde(default)]
    pub can_undo: bool,
    /// Whether a redo step is available.
    #[serde(default)]
    pub can_redo: bool,
    /// Ids of the nodes that changed (including re-routed arrows).
    #[serde(default)]
    pub changed: Vec<String>,
}

/// Summary of an open figure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FigureInfo {
    /// Figure id.
    pub id: FigureId,
    /// Scene file path.
    pub path: Option<String>,
    /// Number of nodes.
    pub nodes: usize,
    /// Revision counter.
    pub revision: u64,
    /// Unsaved changes.
    pub dirty: bool,
}

/// The extension of a figure scene file (JSON content).
pub const FIGURE_SUFFIX: &str = ".figure";
/// The name scenes had before 2026-10-18; still read and saved in place.
pub const LEGACY_FIGURE_SUFFIX: &str = ".figure.json";

/// Where a save wrote the scene and its SVG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPaths {
    /// The scene file (`<name>.figure`, or a legacy `<name>.figure.json`).
    pub json: String,
    /// `<name>.svg`.
    pub svg: String,
    /// The revision written.
    pub revision: u64,
}

/// The scene file and the SVG for a figure path. New figures are
/// `<stem>.figure`; a path that already ends in `.figure.json` (the name used
/// before 2026-10-18) keeps that name, so saving never creates a second file.
/// `x.svg`, `x.json` and `x` give `x.figure`.
pub fn resolve_figure_paths(path: &Path) -> (PathBuf, PathBuf) {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("figure");
    if let Some(stem) = file_name.strip_suffix(LEGACY_FIGURE_SUFFIX) {
        return (path.to_path_buf(), parent.join(format!("{stem}.svg")));
    }
    let stem = if let Some(s) = file_name.strip_suffix(FIGURE_SUFFIX) {
        s
    } else if let Some(s) = file_name
        .strip_suffix(".svg")
        .or_else(|| file_name.strip_suffix(".json"))
    {
        s
    } else {
        file_name
    };
    (
        parent.join(format!("{stem}{FIGURE_SUFFIX}")),
        parent.join(format!("{stem}.svg")),
    )
}

/// An open figure.
#[derive(Debug, Clone)]
pub struct FigureDocument {
    /// Id.
    pub id: FigureId,
    /// Scene file path.
    pub path: Option<PathBuf>,
    /// The scene.
    pub scene: FigureScene,
    /// Bumped by every applied edit, undo or redo.
    pub revision: u64,
    /// Revision on disk.
    pub saved_revision: u64,
    undo: VecDeque<Delta>,
    redo: VecDeque<Delta>,
    max_history: usize,
}

impl FigureDocument {
    /// An empty figure.
    pub fn new(id: FigureId, width: f64, height: f64) -> Self {
        Self::from_scene(id, None, FigureScene::new(width, height))
    }

    /// A figure holding `scene`.
    pub fn from_scene(id: FigureId, path: Option<PathBuf>, scene: FigureScene) -> Self {
        Self {
            id,
            path,
            scene,
            revision: 0,
            saved_revision: 0,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            max_history: DEFAULT_MAX_HISTORY,
        }
    }

    /// Caps the number of undo steps.
    pub fn set_max_history(&mut self, limit: usize) {
        self.max_history = limit;
        while self.undo.len() > limit {
            self.undo.pop_front();
        }
        while self.redo.len() > limit {
            self.redo.pop_front();
        }
    }

    /// Unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// Summary.
    pub fn info(&self) -> FigureInfo {
        FigureInfo {
            id: self.id,
            path: self.path.as_ref().map(|p| p.display().to_string()),
            nodes: self.scene.nodes.len(),
            revision: self.revision,
            dirty: self.is_dirty(),
        }
    }

    /// Whether undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn result(&self, applied: bool, changed: Vec<String>) -> ApplyResult {
        ApplyResult {
            revision: self.revision,
            applied,
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
            changed,
        }
    }

    fn push_bounded(stack: &mut VecDeque<Delta>, delta: Delta, limit: usize) {
        if limit == 0 {
            return;
        }
        while stack.len() >= limit {
            stack.pop_front();
        }
        stack.push_back(delta);
    }

    /// Applies one operation. A failed operation changes nothing.
    pub fn apply(&mut self, op: &FigureOp) -> Result<ApplyResult, FigureError> {
        match op {
            FigureOp::Undo => {
                let Some(delta) = self.undo.pop_back() else {
                    return Ok(self.result(false, Vec::new()));
                };
                delta.revert(&mut self.scene);
                let changed = delta.changed_ids();
                Self::push_bounded(&mut self.redo, delta, self.max_history);
                self.revision += 1;
                Ok(self.result(true, changed))
            }
            FigureOp::Redo => {
                let Some(delta) = self.redo.pop_back() else {
                    return Ok(self.result(false, Vec::new()));
                };
                delta.reapply(&mut self.scene);
                let changed = delta.changed_ids();
                Self::push_bounded(&mut self.undo, delta, self.max_history);
                self.revision += 1;
                Ok(self.result(true, changed))
            }
            other => {
                let delta = ops::apply(&mut self.scene, other)?;
                if delta.is_empty() {
                    return Ok(self.result(false, Vec::new()));
                }
                let changed = delta.changed_ids();
                Self::push_bounded(&mut self.undo, delta, self.max_history);
                self.redo.clear();
                self.revision += 1;
                Ok(self.result(true, changed))
            }
        }
    }
}

/// Writes the scene JSON and its SVG, then marks the document saved.
fn write_doc(
    doc: &mut FigureDocument,
    embeds: &BTreeMap<String, String>,
) -> Result<SavedPaths, FigureError> {
    let target = doc.path.clone().ok_or(FigureError::NoPath)?;
    let (json_path, svg_path) = resolve_figure_paths(&target);
    if let Some(parent) = json_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|source| FigureError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }
    let json_text = doc.scene.to_file_json()?;
    fs::write(&json_path, json_text).map_err(|source| FigureError::Io {
        path: json_path.display().to_string(),
        source,
    })?;
    let svg_text = render_svg_with(&doc.scene, &SvgExportOptions::default(), embeds);
    fs::write(&svg_path, svg_text).map_err(|source| FigureError::Io {
        path: svg_path.display().to_string(),
        source,
    })?;
    doc.saved_revision = doc.revision;
    Ok(SavedPaths {
        json: json_path.display().to_string(),
        svg: svg_path.display().to_string(),
        revision: doc.revision,
    })
}

#[derive(Default)]
struct StoreState {
    next_id: FigureId,
    active: Option<FigureId>,
    figures: BTreeMap<FigureId, FigureDocument>,
}

impl StoreState {
    fn resolve(&self, id: Option<FigureId>) -> Result<FigureId, FigureError> {
        let id = id.or(self.active).ok_or(FigureError::NoActiveFigure)?;
        if self.figures.contains_key(&id) {
            Ok(id)
        } else {
            Err(FigureError::NoSuchFigure(id))
        }
    }

    fn doc_mut(&mut self, id: Option<FigureId>) -> Result<&mut FigureDocument, FigureError> {
        let id = self.resolve(id)?;
        self.figures
            .get_mut(&id)
            .ok_or(FigureError::NoSuchFigure(id))
    }

    fn add(&mut self, path: Option<PathBuf>, scene: FigureScene) -> FigureId {
        self.next_id += 1;
        let id = self.next_id;
        self.figures
            .insert(id, FigureDocument::from_scene(id, path, scene));
        self.active = Some(id);
        id
    }
}

/// Thread-safe store of open figures.
#[derive(Default)]
pub struct FigureStore {
    state: Mutex<StoreState>,
}

impl FigureStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// A new untitled figure, made active.
    pub fn create(&self, width: Option<f64>, height: Option<f64>) -> FigureId {
        let w = width.unwrap_or(DEFAULT_SCENE_WIDTH);
        let h = height.unwrap_or(DEFAULT_SCENE_HEIGHT);
        self.state.lock().add(None, FigureScene::new(w, h))
    }

    /// A new figure at `path`, written to disk at once. Refuses to overwrite.
    pub fn create_at(
        &self,
        path: &Path,
        width: Option<f64>,
        height: Option<f64>,
    ) -> Result<(FigureId, SavedPaths), FigureError> {
        let (json_path, _) = resolve_figure_paths(path);
        if json_path.exists() {
            return Err(FigureError::AlreadyExists(json_path.display().to_string()));
        }
        let w = width.unwrap_or(DEFAULT_SCENE_WIDTH);
        let h = height.unwrap_or(DEFAULT_SCENE_HEIGHT);
        let mut state = self.state.lock();
        let id = state.add(Some(json_path), FigureScene::new(w, h));
        let saved = match state
            .doc_mut(Some(id))
            .and_then(|doc| write_doc(doc, &BTreeMap::new()))
        {
            Ok(saved) => saved,
            Err(error) => {
                state.figures.remove(&id);
                state.active = state.figures.keys().next_back().copied();
                return Err(error);
            }
        };
        Ok((id, saved))
    }

    /// The id of the figure open at `path` (any spelling), if any.
    pub fn find(&self, path: &Path) -> Option<FigureId> {
        let (json_path, _) = resolve_figure_paths(path);
        let state = self.state.lock();
        state
            .figures
            .values()
            .find(|d| d.path.as_deref() == Some(json_path.as_path()))
            .map(|d| d.id)
    }

    /// Opens a figure from disk and makes it active.
    pub fn open(&self, path: &Path) -> Result<FigureId, FigureError> {
        let (json_path, _) = resolve_figure_paths(path);
        let target = if path.exists() && path.extension().is_some_and(|e| e == "json") {
            path.to_path_buf()
        } else if json_path.exists() {
            json_path
        } else {
            return Err(FigureError::Io {
                path: path.display().to_string(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
            });
        };
        let text = fs::read_to_string(&target).map_err(|source| FigureError::Io {
            path: target.display().to_string(),
            source,
        })?;
        let scene = FigureScene::from_json(&text)?;
        let (canonical, _) = resolve_figure_paths(&target);
        Ok(self.state.lock().add(Some(canonical), scene))
    }

    /// The figure at `path`, opening it when it is not open yet.
    pub fn find_or_open(&self, path: &Path) -> Result<FigureId, FigureError> {
        match self.find(path) {
            Some(id) => Ok(id),
            None => self.open(path),
        }
    }

    /// Saves scene JSON and SVG. `path` moves the figure there first.
    pub fn save(
        &self,
        id: Option<FigureId>,
        path: Option<&Path>,
        embeds: &BTreeMap<String, String>,
    ) -> Result<SavedPaths, FigureError> {
        let mut state = self.state.lock();
        let doc = state.doc_mut(id)?;
        if let Some(p) = path {
            doc.path = Some(resolve_figure_paths(p).0);
        }
        write_doc(doc, embeds)
    }

    /// Makes a figure active.
    pub fn activate(&self, id: FigureId) -> Result<(), FigureError> {
        let mut state = self.state.lock();
        let id = state.resolve(Some(id))?;
        state.active = Some(id);
        Ok(())
    }

    /// Closes a figure; false when it was not open.
    pub fn close(&self, id: FigureId) -> Result<bool, FigureError> {
        let mut state = self.state.lock();
        if state.figures.remove(&id).is_some() {
            if state.active == Some(id) {
                state.active = state.figures.keys().next_back().copied();
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Applies an operation to the targeted or active figure.
    pub fn apply(&self, id: Option<FigureId>, op: &FigureOp) -> Result<ApplyResult, FigureError> {
        self.state.lock().doc_mut(id)?.apply(op)
    }

    /// The resolved id of the targeted or active figure.
    pub fn resolve(&self, id: Option<FigureId>) -> Result<FigureId, FigureError> {
        self.state.lock().resolve(id)
    }

    /// Runs `f` on a document under the store lock.
    pub fn with_doc<T>(
        &self,
        id: Option<FigureId>,
        f: impl FnOnce(&mut FigureDocument) -> Result<T, FigureError>,
    ) -> Result<T, FigureError> {
        let mut state = self.state.lock();
        f(state.doc_mut(id)?)
    }

    /// A copy of the scene.
    pub fn get(&self, id: Option<FigureId>) -> Result<FigureScene, FigureError> {
        Ok(self.state.lock().doc_mut(id)?.scene.clone())
    }

    /// The scene as SVG.
    pub fn export_svg(
        &self,
        id: Option<FigureId>,
        options: &SvgExportOptions,
        embeds: &BTreeMap<String, String>,
    ) -> Result<String, FigureError> {
        let mut state = self.state.lock();
        let doc = state.doc_mut(id)?;
        Ok(render_svg_with(&doc.scene, options, embeds))
    }

    /// The topmost node at `point`.
    pub fn hit_test(
        &self,
        id: Option<FigureId>,
        point: Vec2,
    ) -> Result<Option<FigureNode>, FigureError> {
        let mut state = self.state.lock();
        Ok(hit_test(&state.doc_mut(id)?.scene, point).cloned())
    }

    /// Every open figure.
    pub fn list(&self) -> Vec<FigureInfo> {
        self.state
            .lock()
            .figures
            .values()
            .map(FigureDocument::info)
            .collect()
    }
}
