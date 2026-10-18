//! In-memory store and lifecycle management for figure scenes with undo/redo.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::FigureError;
use crate::geometry::hit_test;
use crate::scene::{FigureNode, FigureScene, Vec2};
use crate::svg::{SvgExportOptions, render_svg};

/// Default upper bound for in-memory undo/redo history snapshots.
pub const DEFAULT_MAX_HISTORY: usize = 100;

/// Default initial scene width in CSS pixels.
pub const DEFAULT_SCENE_WIDTH: f64 = 800.0;

/// Default initial scene height in CSS pixels.
pub const DEFAULT_SCENE_HEIGHT: f64 = 600.0;

/// Unique identifier for an open figure.
pub type FigureId = u64;

/// Tagged representation of an edit operation on a figure scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum TaggedFigureOp {
    AddNode {
        node: FigureNode,
    },
    UpdateNode {
        node: FigureNode,
    },
    RemoveNode {
        id: String,
    },
    Clear,
    SetStyle {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
    Translate {
        id: String,
        dx: f64,
        dy: f64,
    },
    Undo,
    Redo,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum TypeFigureOp {
    AddNode {
        node: FigureNode,
    },
    UpdateNode {
        node: FigureNode,
    },
    RemoveNode {
        id: String,
    },
    Clear,
    SetStyle {
        id: String,
        #[serde(default)]
        stroke: Option<String>,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        size: Option<f64>,
        #[serde(default)]
        color: Option<String>,
    },
    Translate {
        id: String,
        dx: f64,
        dy: f64,
    },
    Undo,
    Redo,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum KindFigureOp {
    AddNode {
        node: FigureNode,
    },
    UpdateNode {
        node: FigureNode,
    },
    RemoveNode {
        id: String,
    },
    Clear,
    SetStyle {
        id: String,
        #[serde(default)]
        stroke: Option<String>,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        size: Option<f64>,
        #[serde(default)]
        color: Option<String>,
    },
    Translate {
        id: String,
        dx: f64,
        dy: f64,
    },
    Undo,
    Redo,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
enum OpHelper {
    TaggedOp(TaggedFigureOp),
    TaggedType(TypeFigureOp),
    TaggedKind(KindFigureOp),
    Str(String),
}

/// Tagged enum of edit operations on a figure scene.
#[derive(Debug, Clone, PartialEq)]
pub enum FigureOp {
    /// Add a new node to the scene.
    AddNode {
        /// The node to append.
        node: FigureNode,
    },
    /// Update an existing node matching `node.id()`.
    UpdateNode {
        /// Replacement node.
        node: FigureNode,
    },
    /// Remove a node by its ID.
    RemoveNode {
        /// ID of node to remove.
        id: String,
    },
    /// Clear all nodes from the scene.
    Clear,
    /// Update styles of a node by ID.
    SetStyle {
        /// ID of target node.
        id: String,
        /// Optional replacement stroke color.
        stroke: Option<String>,
        /// Optional replacement fill color.
        fill: Option<String>,
        /// Optional replacement size.
        size: Option<f64>,
        /// Optional replacement color.
        color: Option<String>,
    },
    /// Move a node by a delta in scene units (one undo step).
    Translate {
        /// ID of the node to move.
        id: String,
        /// Horizontal offset.
        dx: f64,
        /// Vertical offset.
        dy: f64,
    },
    /// Undo the latest operation.
    Undo,
    /// Redo the latest undone operation.
    Redo,
}

impl Serialize for FigureOp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let tagged = match self {
            Self::AddNode { node } => TaggedFigureOp::AddNode { node: node.clone() },
            Self::UpdateNode { node } => TaggedFigureOp::UpdateNode { node: node.clone() },
            Self::RemoveNode { id } => TaggedFigureOp::RemoveNode { id: id.clone() },
            Self::Clear => TaggedFigureOp::Clear,
            Self::SetStyle {
                id,
                stroke,
                fill,
                size,
                color,
            } => TaggedFigureOp::SetStyle {
                id: id.clone(),
                stroke: stroke.clone(),
                fill: fill.clone(),
                size: *size,
                color: color.clone(),
            },
            Self::Translate { id, dx, dy } => TaggedFigureOp::Translate {
                id: id.clone(),
                dx: *dx,
                dy: *dy,
            },
            Self::Undo => TaggedFigureOp::Undo,
            Self::Redo => TaggedFigureOp::Redo,
        };
        tagged.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FigureOp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = OpHelper::deserialize(deserializer)?;
        match helper {
            OpHelper::TaggedOp(op) => Ok(match op {
                TaggedFigureOp::AddNode { node } => Self::AddNode { node },
                TaggedFigureOp::UpdateNode { node } => Self::UpdateNode { node },
                TaggedFigureOp::RemoveNode { id } => Self::RemoveNode { id },
                TaggedFigureOp::Clear => Self::Clear,
                TaggedFigureOp::SetStyle {
                    id,
                    stroke,
                    fill,
                    size,
                    color,
                } => Self::SetStyle {
                    id,
                    stroke,
                    fill,
                    size,
                    color,
                },
                TaggedFigureOp::Translate { id, dx, dy } => Self::Translate { id, dx, dy },
                TaggedFigureOp::Undo => Self::Undo,
                TaggedFigureOp::Redo => Self::Redo,
            }),
            OpHelper::TaggedType(op) => Ok(match op {
                TypeFigureOp::AddNode { node } => Self::AddNode { node },
                TypeFigureOp::UpdateNode { node } => Self::UpdateNode { node },
                TypeFigureOp::RemoveNode { id } => Self::RemoveNode { id },
                TypeFigureOp::Clear => Self::Clear,
                TypeFigureOp::SetStyle {
                    id,
                    stroke,
                    fill,
                    size,
                    color,
                } => Self::SetStyle {
                    id,
                    stroke,
                    fill,
                    size,
                    color,
                },
                TypeFigureOp::Translate { id, dx, dy } => Self::Translate { id, dx, dy },
                TypeFigureOp::Undo => Self::Undo,
                TypeFigureOp::Redo => Self::Redo,
            }),
            OpHelper::TaggedKind(op) => Ok(match op {
                KindFigureOp::AddNode { node } => Self::AddNode { node },
                KindFigureOp::UpdateNode { node } => Self::UpdateNode { node },
                KindFigureOp::RemoveNode { id } => Self::RemoveNode { id },
                KindFigureOp::Clear => Self::Clear,
                KindFigureOp::SetStyle {
                    id,
                    stroke,
                    fill,
                    size,
                    color,
                } => Self::SetStyle {
                    id,
                    stroke,
                    fill,
                    size,
                    color,
                },
                KindFigureOp::Translate { id, dx, dy } => Self::Translate { id, dx, dy },
                KindFigureOp::Undo => Self::Undo,
                KindFigureOp::Redo => Self::Redo,
            }),
            OpHelper::Str(s) => match s.to_ascii_lowercase().as_str() {
                "undo" => Ok(Self::Undo),
                "redo" => Ok(Self::Redo),
                "clear" => Ok(Self::Clear),
                other => Err(serde::de::Error::custom(format!(
                    "unknown string figure operation `{other}`"
                ))),
            },
        }
    }
}

/// Result returned from applying an operation to a figure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyResult {
    /// The updated revision counter.
    pub revision: u64,
    /// Whether an edit, undo, or redo was actually applied.
    pub applied: bool,
    /// Whether an undo step is available after this operation.
    #[serde(default)]
    pub can_undo: bool,
    /// Whether a redo step is available after this operation.
    #[serde(default)]
    pub can_redo: bool,
}

/// Metadata summary of an open figure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FigureInfo {
    /// Figure identifier.
    pub id: FigureId,
    /// Associated filesystem path if saved/opened.
    pub path: Option<String>,
    /// Number of nodes in the scene.
    pub nodes: usize,
    /// Monotonically increasing revision counter.
    pub revision: u64,
    /// Whether the figure has unsaved changes.
    pub dirty: bool,
}

/// Where a save wrote the scene and its rendered SVG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPaths {
    /// The scene JSON (`<name>.figure.json`).
    pub json: String,
    /// The rendered SVG next to it (`<name>.svg`).
    pub svg: String,
    /// The revision that was written.
    pub revision: u64,
}

/// Writes the scene JSON and its SVG, then marks the document saved.
fn write_doc(doc: &mut FigureDocument) -> Result<SavedPaths, FigureError> {
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

    // Pretty-printed with a final newline: the scene is a project file that
    // people diff and commit.
    let mut json_text = serde_json::to_string_pretty(&doc.scene)?;
    json_text.push('\n');
    fs::write(&json_path, json_text).map_err(|source| FigureError::Io {
        path: json_path.display().to_string(),
        source,
    })?;

    let svg_text = render_svg(&doc.scene, &SvgExportOptions::default());
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

/// Resolves canonical file paths for figure JSON and companion headless SVG.
pub fn resolve_figure_paths(path: &Path) -> (PathBuf, PathBuf) {
    let path_str = path.to_string_lossy();
    let parent = path.parent().unwrap_or_else(|| Path::new(""));

    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("figure");

    let stem = if path_str.ends_with(".figure.json") {
        file_name.strip_suffix(".figure.json").unwrap_or(file_name)
    } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if ext == "svg" || ext == "json" {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("figure")
        } else {
            file_name
        }
    } else {
        file_name
    };

    let json_name = format!("{stem}.figure.json");
    let svg_name = format!("{stem}.svg");

    let json_path = if parent.as_os_str().is_empty() {
        PathBuf::from(json_name)
    } else {
        parent.join(json_name)
    };
    let svg_path = if parent.as_os_str().is_empty() {
        PathBuf::from(svg_name)
    } else {
        parent.join(svg_name)
    };

    (json_path, svg_path)
}

/// An open figure document with revision tracking and bounded undo/redo stacks.
#[derive(Debug, Clone)]
pub struct FigureDocument {
    /// Document identifier.
    pub id: FigureId,
    /// Associated filesystem path.
    pub path: Option<PathBuf>,
    /// The current scene graph.
    pub scene: FigureScene,
    /// Revision counter incremented on each applied edit, undo, or redo.
    pub revision: u64,
    /// Revision counter at last save.
    pub saved_revision: u64,
    undo_stack: Vec<FigureScene>,
    redo_stack: Vec<FigureScene>,
    max_history: usize,
}

impl FigureDocument {
    /// Creates a new empty figure document with given dimensions.
    pub fn new(id: FigureId, width: f64, height: f64) -> Self {
        Self {
            id,
            path: None,
            scene: FigureScene::new(width, height),
            revision: 0,
            saved_revision: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_history: DEFAULT_MAX_HISTORY,
        }
    }

    /// Creates a figure document from an existing scene.
    pub fn from_scene(id: FigureId, path: Option<PathBuf>, scene: FigureScene) -> Self {
        Self {
            id,
            path,
            scene,
            revision: 0,
            saved_revision: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_history: DEFAULT_MAX_HISTORY,
        }
    }

    /// Sets the maximum number of undo steps preserved in memory.
    pub fn set_max_history(&mut self, limit: usize) {
        self.max_history = limit;
        while self.undo_stack.len() > self.max_history {
            self.undo_stack.remove(0);
        }
        while self.redo_stack.len() > self.max_history {
            self.redo_stack.remove(0);
        }
    }

    /// Returns true if the figure has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// Returns a metadata snapshot of the figure.
    pub fn info(&self) -> FigureInfo {
        FigureInfo {
            id: self.id,
            path: self.path.as_ref().map(|p| p.display().to_string()),
            nodes: self.scene.nodes.len(),
            revision: self.revision,
            dirty: self.is_dirty(),
        }
    }

    fn push_undo(&mut self) {
        if self.undo_stack.len() >= self.max_history {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(self.scene.clone());
        self.redo_stack.clear();
    }

    /// Whether an undo step is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether a redo step is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    fn result(&self, applied: bool) -> ApplyResult {
        ApplyResult {
            revision: self.revision,
            applied,
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
        }
    }

    /// Records an applied edit: bumps the revision and reports the new state.
    fn edited(&mut self) -> ApplyResult {
        self.revision += 1;
        self.result(true)
    }

    /// Applies an operation to the figure, updating revision and undo/redo stacks.
    ///
    /// A failed operation leaves the scene, the revision and the history as
    /// they were.
    pub fn apply(&mut self, op: FigureOp) -> Result<ApplyResult, FigureError> {
        match op {
            FigureOp::Undo => {
                let Some(prev) = self.undo_stack.pop() else {
                    return Ok(self.result(false));
                };
                if self.redo_stack.len() >= self.max_history {
                    self.redo_stack.remove(0);
                }
                let current = std::mem::replace(&mut self.scene, prev);
                self.redo_stack.push(current);
                Ok(self.edited())
            }
            FigureOp::Redo => {
                let Some(next) = self.redo_stack.pop() else {
                    return Ok(self.result(false));
                };
                if self.undo_stack.len() >= self.max_history {
                    self.undo_stack.remove(0);
                }
                let current = std::mem::replace(&mut self.scene, next);
                self.undo_stack.push(current);
                Ok(self.edited())
            }
            FigureOp::AddNode { node } => {
                if self.scene.node_by_id(node.id()).is_some() {
                    return Err(FigureError::DuplicateNode(node.id().to_owned()));
                }
                self.push_undo();
                self.scene.append_node(node);
                Ok(self.edited())
            }
            FigureOp::UpdateNode { node } => {
                if self.scene.node_by_id(node.id()).is_none() {
                    return Err(FigureError::NodeNotFound(node.id().to_owned()));
                }
                self.push_undo();
                self.scene.replace_node(node);
                Ok(self.edited())
            }
            FigureOp::RemoveNode { id } => {
                if self.scene.node_by_id(&id).is_none() {
                    return Err(FigureError::NodeNotFound(id));
                }
                self.push_undo();
                self.scene.remove_node(&id);
                Ok(self.edited())
            }
            FigureOp::Translate { id, dx, dy } => {
                if !dx.is_finite() || !dy.is_finite() {
                    return Err(FigureError::InvalidArgument(
                        "`dx` and `dy` must be finite numbers".to_owned(),
                    ));
                }
                if self.scene.node_by_id(&id).is_none() {
                    return Err(FigureError::NodeNotFound(id));
                }
                if dx == 0.0 && dy == 0.0 {
                    return Ok(self.result(false));
                }
                self.push_undo();
                if let Some(node) = self.scene.nodes.iter_mut().find(|n| n.id() == id) {
                    node.translate(dx, dy);
                }
                Ok(self.edited())
            }
            FigureOp::Clear => {
                self.push_undo();
                self.scene.nodes.clear();
                Ok(self.edited())
            }
            FigureOp::SetStyle {
                id,
                stroke,
                fill,
                size,
                color,
            } => {
                if self.scene.node_by_id(&id).is_none() {
                    return Err(FigureError::NodeNotFound(id));
                }
                self.push_undo();
                if let Some(node) = self.scene.nodes.iter_mut().find(|n| n.id() == id) {
                    apply_style(node, stroke, fill, size, color);
                }
                Ok(self.edited())
            }
        }
    }
}

fn apply_style(
    node: &mut FigureNode,
    stroke: Option<String>,
    fill: Option<String>,
    size: Option<f64>,
    color: Option<String>,
) {
    match node {
        FigureNode::Stroke(s) => {
            if let Some(c) = color.or(stroke) {
                s.color = c;
            }
            if let Some(sz) = size {
                s.size = sz;
            }
        }
        FigureNode::Rect(r) => {
            if let Some(s) = stroke.or(color) {
                r.stroke = s;
            }
            if fill.is_some() {
                r.fill = fill;
            }
        }
        FigureNode::Ellipse(e) => {
            if let Some(s) = stroke.or(color) {
                e.stroke = s;
            }
            if fill.is_some() {
                e.fill = fill;
            }
        }
        FigureNode::Arrow(a) => {
            if let Some(c) = color.or(stroke) {
                a.color = c;
            }
        }
        FigureNode::Math(m) => {
            if let Some(c) = color.or(stroke) {
                m.color = c;
            }
            if let Some(sz) = size {
                m.size = sz;
            }
        }
    }
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

    fn doc_mut(
        &mut self,
        id: Option<FigureId>,
    ) -> Result<(FigureId, &mut FigureDocument), FigureError> {
        let id = self.resolve(id)?;
        let doc = self
            .figures
            .get_mut(&id)
            .ok_or(FigureError::NoSuchFigure(id))?;
        Ok((id, doc))
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

/// In-memory store of open figures with thread-safe access.
#[derive(Default)]
pub struct FigureStore {
    state: Mutex<StoreState>,
}

impl FigureStore {
    /// Creates a new empty figure store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new untitled figure and makes it active.
    pub fn create(&self, width: Option<f64>, height: Option<f64>) -> FigureId {
        let w = width.unwrap_or(DEFAULT_SCENE_WIDTH);
        let h = height.unwrap_or(DEFAULT_SCENE_HEIGHT);
        self.state.lock().add(None, FigureScene::new(w, h))
    }

    /// Creates a figure at `path` and writes it to disk straight away
    /// (`<name>.figure.json` and `<name>.svg`), so it exists as a project file
    /// before the first edit. Refuses to overwrite an existing figure.
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
        let saved = match state.doc_mut(Some(id)).and_then(|(_, doc)| write_doc(doc)) {
            Ok(saved) => saved,
            Err(error) => {
                // Nothing usable was created: do not leave a pathless ghost open.
                state.figures.remove(&id);
                state.active = state.figures.keys().next_back().copied();
                return Err(error);
            }
        };
        Ok((id, saved))
    }

    /// Opens a figure from disk, resolving companion JSON and making it active.
    pub fn open(&self, path: &Path) -> Result<FigureId, FigureError> {
        let (json_path, _) = resolve_figure_paths(path);
        let target = if path.exists() {
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
        let (canonical_json, _) = resolve_figure_paths(&target);
        Ok(self.state.lock().add(Some(canonical_json), scene))
    }

    /// Saves a figure, writing both `<name>.figure.json` and `<name>.svg`.
    ///
    /// `path` (any of `x.figure.json`, `x.svg` or `x`) moves the figure there
    /// first, so a renamed file is saved under its new name.
    pub fn save(
        &self,
        id: Option<FigureId>,
        path: Option<&Path>,
    ) -> Result<SavedPaths, FigureError> {
        let mut state = self.state.lock();
        let (_, doc) = state.doc_mut(id)?;
        if let Some(p) = path {
            let (json_path, _) = resolve_figure_paths(p);
            doc.path = Some(json_path);
        }
        write_doc(doc)
    }

    /// Activates an open figure by ID.
    pub fn activate(&self, id: FigureId) -> Result<(), FigureError> {
        let mut state = self.state.lock();
        let id = state.resolve(Some(id))?;
        state.active = Some(id);
        Ok(())
    }

    /// Closes an open figure by ID.
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
    pub fn apply(&self, id: Option<FigureId>, op: FigureOp) -> Result<ApplyResult, FigureError> {
        let mut state = self.state.lock();
        let (_, doc) = state.doc_mut(id)?;
        doc.apply(op)
    }

    /// Returns a clone of the scene graph for the targeted or active figure.
    pub fn get(&self, id: Option<FigureId>) -> Result<FigureScene, FigureError> {
        let mut state = self.state.lock();
        let (_, doc) = state.doc_mut(id)?;
        Ok(doc.scene.clone())
    }

    /// Exports the targeted or active figure to SVG markup.
    pub fn export_svg(
        &self,
        id: Option<FigureId>,
        options: Option<&SvgExportOptions>,
    ) -> Result<String, FigureError> {
        let default_opts = SvgExportOptions::default();
        let opts = options.unwrap_or(&default_opts);
        let mut state = self.state.lock();
        let (_, doc) = state.doc_mut(id)?;
        Ok(render_svg(&doc.scene, opts))
    }

    /// Performs a hit test on the targeted or active figure scene.
    pub fn hit_test(
        &self,
        id: Option<FigureId>,
        point: Vec2,
    ) -> Result<Option<FigureNode>, FigureError> {
        let mut state = self.state.lock();
        let (_, doc) = state.doc_mut(id)?;
        Ok(hit_test(&doc.scene, point).cloned())
    }

    /// Returns metadata summaries for all open figures.
    pub fn list(&self) -> Vec<FigureInfo> {
        let state = self.state.lock();
        state.figures.values().map(FigureDocument::info).collect()
    }
}
