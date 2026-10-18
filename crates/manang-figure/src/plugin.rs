//! Kernel plugin interface for the figure studio.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use manang_kernel::{Context, EventBus, Hook, Manifest, Payload, Plugin, PluginError};
use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::error::FigureError;
use crate::math::{self, MathError, MathSvg};
use crate::ops::FigureOp;
use crate::palette::Theme;
use crate::scene::{FigureNode, FigureScene, MathLanguage, MathNode, ShapeNode, Vec2};
use crate::store::{FigureId, FigureStore, resolve_figure_paths};
use crate::svg::{SvgExportOptions, render_svg_with};
use crate::tools::{self, Target, ToolError};

const MANIFEST: &str = include_str!("../manifest.toml");

/// The figure plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, manang_kernel::ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// The figure plugin with its own store and no event bus.
pub fn plugin() -> impl Plugin {
    FigurePlugin::new()
}

/// The figure plugin sharing `store`.
pub fn plugin_with_store(store: Arc<FigureStore>) -> impl Plugin {
    FigurePlugin::with_store(store)
}

/// The figure plugin announcing tool edits (`figure.changed`) on `events`.
pub fn plugin_with_events(events: EventBus) -> impl Plugin {
    FigurePlugin::new().with_events(events)
}

/// Rendered formulas kept between calls: `(language, source, size)` to the
/// rendering. Saving, exporting and the canvas ask for the same formulas
/// again and again; a render is milliseconds, a lookup is not. Cleared when
/// it grows past [`MATH_CACHE_LIMIT`].
type MathCache = Arc<Mutex<HashMap<(MathLanguage, String, u64), MathSvg>>>;

const MATH_CACHE_LIMIT: usize = 256;

/// Exposes a [`FigureStore`] as kernel commands and agent tools.
#[derive(Clone)]
pub struct FigurePlugin {
    store: Arc<FigureStore>,
    events: Option<EventBus>,
    math: MathCache,
}

impl Default for FigurePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl FigurePlugin {
    /// A plugin with an empty store.
    pub fn new() -> Self {
        Self::with_store(Arc::new(FigureStore::new()))
    }

    /// A plugin over an existing store.
    pub fn with_store(store: Arc<FigureStore>) -> Self {
        Self {
            store,
            events: None,
            math: MathCache::default(),
        }
    }

    /// Announce edits made by tools on `events`.
    pub fn with_events(mut self, events: EventBus) -> Self {
        self.events = Some(events);
        self
    }

    /// The store.
    pub fn store(&self) -> &Arc<FigureStore> {
        &self.store
    }
}

fn args(input: Payload) -> Result<Value, PluginError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(PluginError::Failed(
            "[invalid_argument] expected a JSON payload".to_owned(),
        )),
    }
}

fn id(args: &Value) -> Result<Option<FigureId>, PluginError> {
    match args.get("id") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| PluginError::Failed("`id` must be an unsigned integer".to_owned())),
    }
}

fn number_f64(args: &Value, key: &str) -> Result<f64, PluginError> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| PluginError::Failed(format!("`{key}` must be a number")))
}

fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, PluginError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::Failed(format!("`{key}` must be a string")))
}

fn failed(error: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(error.to_string())
}

fn tool_failed(error: ToolError) -> PluginError {
    PluginError::Failed(error.to_string())
}

/// The PDF next to `svg` through `project.export-pdf`, when the project
/// includes PDFs; None otherwise or without a project plugin.
fn pdf_for(ctx: &Context<'_>, svg: &str, force: bool) -> Result<Option<String>, PluginError> {
    const COMMAND: &str = "project.export-pdf";
    if !ctx.has_command(COMMAND) {
        return Ok(None);
    }
    let mut request = json!({ "path": svg });
    if force {
        request["force"] = json!(true);
    }
    match ctx.invoke(COMMAND, Payload::Json(request)) {
        Ok(Payload::Json(out)) => Ok(out.get("pdf").and_then(Value::as_str).map(str::to_owned)),
        Ok(_) => Ok(None),
        Err(e) => Err(PluginError::Failed(format!(
            "saved {svg}, but its PDF could not be written: {e}"
        ))),
    }
}

/// Typst math through the Typst plugin (`typst.math-to-svg`).
fn typst_math(ctx: &Context<'_>, source: &str, size: f64) -> Result<MathSvg, MathError> {
    const COMMAND: &str = "typst.math-to-svg";
    if !ctx.has_command(COMMAND) {
        return Err(MathError::new(
            "the Typst plugin is not available, so Typst formulas cannot be rendered",
        ));
    }
    let answer = ctx.invoke(
        COMMAND,
        Payload::Json(json!({ "source": source, "size": size })),
    );
    match answer {
        Ok(Payload::Binary(bytes)) => {
            let svg = String::from_utf8(bytes.to_vec())
                .map_err(|_| MathError::new("the Typst plugin answered with invalid text"))?;
            let (width, height) =
                math::svg_size(&svg).ok_or_else(|| MathError::new("the rendering has no size"))?;
            Ok(MathSvg { svg, width, height })
        }
        Ok(_) => Err(MathError::new("the Typst plugin answered without an SVG")),
        Err(e) => {
            let text = e.to_string();
            Err(MathError::from_tagged(&text).unwrap_or_else(|| MathError::new(text)))
        }
    }
}

impl FigurePlugin {
    /// Renders a formula, through the cache.
    pub fn render_formula(
        &self,
        ctx: &Context<'_>,
        language: MathLanguage,
        source: &str,
        size: f64,
    ) -> Result<MathSvg, MathError> {
        let key = (language, source.to_owned(), size.to_bits());
        if let Some(hit) = self.math.lock().get(&key) {
            return Ok(hit.clone());
        }
        let rendered = math::render_math(language, source, size, |s, z| typst_math(ctx, s, z))?;
        let mut cache = self.math.lock();
        if cache.len() >= MATH_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, rendered.clone());
        Ok(rendered)
    }

    /// Renderings of every formula in `scene`, keyed by node id. Formulas
    /// that fail draw as a placeholder.
    fn math_embeds(&self, ctx: &Context<'_>, scene: &FigureScene) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for node in &scene.nodes {
            if let FigureNode::Math(m) = node
                && let Ok(rendered) = self.render_formula(ctx, m.language, &m.source, m.size)
            {
                out.insert(m.id.clone(), rendered.svg);
            }
        }
        out
    }

    /// Renders new and changed formulas of an operation and records their
    /// measured box, so selection and arrow binding know their size.
    fn measure_formulas(
        &self,
        ctx: &Context<'_>,
        scene: &FigureScene,
        op: &mut FigureOp,
    ) -> Result<(), ToolError> {
        let fail = |id: &str, e: MathError| ToolError {
            code: "math_error",
            message: format!("formula `{id}`: {e}"),
        };
        match op {
            FigureOp::Add { nodes, .. } => {
                for node in nodes {
                    if let FigureNode::Math(m) = node {
                        let r = self
                            .render_formula(ctx, m.language, &m.source, m.size)
                            .map_err(|e| fail(&m.id, e))?;
                        m.width = r.width;
                        m.height = r.height;
                    }
                }
            }
            FigureOp::Batch { ops } => {
                for op in ops {
                    self.measure_formulas(ctx, scene, op)?;
                }
            }
            FigureOp::Update { id, set } => {
                let Some(FigureNode::Math(current)) = scene.node_by_id(id) else {
                    return Ok(());
                };
                if !["source", "language", "size"]
                    .iter()
                    .any(|k| set.contains_key(*k))
                {
                    return Ok(());
                }
                let mut next: MathNode = current.clone();
                if let Some(s) = set.get("source").and_then(Value::as_str) {
                    s.clone_into(&mut next.source);
                }
                if let Some(l) = set.get("language").and_then(Value::as_str)
                    && let Ok(l) = l.parse()
                {
                    next.language = l;
                }
                if let Some(z) = set.get("size").and_then(Value::as_f64) {
                    next.size = z;
                }
                let r = self
                    .render_formula(ctx, next.language, &next.source, next.size)
                    .map_err(|e| fail(id, e))?;
                set.insert("width".to_owned(), json!(r.width));
                set.insert("height".to_owned(), json!(r.height));
            }
            _ => {}
        }
        Ok(())
    }
}

/// Mermaid blocks rendered by the diagram plugin, for export. Blocks that
/// fail (or no diagram plugin) draw as a labelled placeholder.
fn mermaid_embeds(
    ctx: &Context<'_>,
    scene: &FigureScene,
    theme: Theme,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if !ctx.has_command("diagram.render") {
        return out;
    }
    let theme = match theme {
        Theme::Dark => "dark",
        Theme::Light => "light",
        Theme::Print => "print",
    };
    for node in &scene.nodes {
        if let FigureNode::Mermaid(s) = node
            && let Some(source) = &s.source
            && let Ok(Payload::Binary(bytes)) = ctx.invoke(
                "diagram.render",
                Payload::Json(json!({ "language": "mermaid", "source": source, "theme": theme })),
            )
            && let Ok(svg) = String::from_utf8(bytes.to_vec())
        {
            out.insert(s.id.clone(), svg);
        }
    }
    out
}

impl FigurePlugin {
    /// Everything the renderer draws from other plugins: Mermaid blocks and
    /// formulas, keyed by node id.
    fn embeds_for(
        &self,
        ctx: &Context<'_>,
        scene: &FigureScene,
        theme: Theme,
    ) -> BTreeMap<String, String> {
        let mut out = mermaid_embeds(ctx, scene, theme);
        out.extend(self.math_embeds(ctx, scene));
        out
    }

    fn target(&self, a: &Value) -> Result<FigureId, ToolError> {
        let target: Target = serde_json::from_value(json!({
            "figure": a.get("figure"),
            "path": a.get("path"),
        }))
        .map_err(|e| ToolError {
            code: "invalid_argument",
            message: format!("`figure` must be an id and `path` a string: {e}"),
        })?;
        match (target.figure, target.path) {
            (Some(id), _) => Ok(self.store.resolve(Some(id))?),
            (None, Some(path)) => Ok(self.store.find_or_open(Path::new(&path))?),
            (None, None) => Ok(self.store.resolve(None)?),
        }
    }

    fn figure_ref(&self, id: FigureId) -> Value {
        self.store.list().into_iter().find(|f| f.id == id).map_or(
            Value::Null,
            |f| json!({ "id": f.id, "path": f.path, "revision": f.revision }),
        )
    }

    /// Applies a tool's operation, announces it and answers with the ids.
    fn commit(&self, id: FigureId, op: &FigureOp, ids: Vec<String>) -> Result<Value, ToolError> {
        let result = self.store.apply(Some(id), op)?;
        if result.applied
            && let Some(events) = &self.events
        {
            events.emit(
                "figure.changed",
                Payload::Json(json!({ "id": id, "revision": result.revision })),
            );
        }
        Ok(json!({ "figure": self.figure_ref(id), "ids": ids }))
    }

    fn tool(&self, ctx: &Context<'_>, command: &str, a: &Value) -> Result<Value, ToolError> {
        let id = self.target(a)?;
        let scene = self.store.get(Some(id))?;
        match command {
            "figure.describe" => {
                let mut out = tools::describe(&scene);
                out["figure"] = self.figure_ref(id);
                Ok(out)
            }
            "figure.add-shapes" => {
                let (mut op, ids) = tools::add_shapes(&scene, a)?;
                self.measure_formulas(ctx, &scene, &mut op)?;
                let shapes: Vec<Value> = match &op {
                    FigureOp::Add { nodes, .. } => nodes.iter().map(tools::shape_summary).collect(),
                    _ => Vec::new(),
                };
                let mut out = self.commit(id, &op, ids)?;
                out["shapes"] = json!(shapes);
                Ok(out)
            }
            "figure.connect" => {
                let (op, ids) = tools::connect(&scene, a)?;
                self.commit(id, &op, ids)
            }
            "figure.update" => {
                let (mut op, ids) = tools::update(&scene, a)?;
                self.measure_formulas(ctx, &scene, &mut op)?;
                self.commit(id, &op, ids)
            }
            "figure.remove" => {
                let (op, ids) = tools::remove(&scene, a)?;
                self.commit(id, &op, ids)
            }
            "figure.layout" => {
                let (op, ids) = tools::layout(&scene, a)?;
                self.commit(id, &op, ids)
            }
            "figure.style" => {
                let (op, ids) = tools::style(&scene, a)?;
                self.commit(id, &op, ids)
            }
            "figure.to-mermaid" => {
                let (source, skipped) = crate::mermaid::to_mermaid(&scene);
                Ok(json!({ "figure": self.figure_ref(id), "source": source, "skipped": skipped }))
            }
            "figure.from-mermaid" => self.insert_mermaid(ctx, id, &scene, a),
            "figure.export" => self.export(ctx, id, &scene, a),
            other => Err(ToolError {
                code: "invalid_argument",
                message: format!("unknown tool command `{other}`"),
            }),
        }
    }

    fn insert_mermaid(
        &self,
        ctx: &Context<'_>,
        id: FigureId,
        scene: &FigureScene,
        a: &Value,
    ) -> Result<Value, ToolError> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            source: String,
            #[serde(default)]
            x: Option<f64>,
            #[serde(default)]
            y: Option<f64>,
            #[serde(default)]
            mode: Option<String>,
        }
        let input: Input = tools::parse(&tools::strip_target(a))?;
        if input.source.trim().is_empty() {
            return Err(ToolError {
                code: "invalid_argument",
                message: "`source` is empty".to_owned(),
            });
        }
        let mode = input.mode.as_deref().unwrap_or("auto");
        if !matches!(mode, "auto" | "shapes" | "block") {
            return Err(ToolError {
                code: "invalid_argument",
                message: format!("unknown mode `{mode}`; use auto, shapes or block"),
            });
        }
        let origin = tools::insertion_point(scene, input.x, input.y);
        let mut counter = scene.highest_counter();
        let mut fallback = None;
        let mut family = None;
        if mode != "block" {
            let laid_out = if ctx.has_command("diagram.layout") {
                ctx.invoke(
                    "diagram.layout",
                    Payload::Json(json!({ "source": input.source })),
                )
                .map_err(|e| e.to_string())
            } else {
                Err("the diagram plugin is not available".to_owned())
            };
            match laid_out {
                Ok(Payload::Json(layout)) => {
                    family = layout
                        .get("family")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let nodes = crate::mermaid::nodes_from_layout(&layout, origin, &mut counter)?;
                    let ids: Vec<String> = nodes.iter().map(|n| n.id().to_owned()).collect();
                    let mut out = self.commit(id, &FigureOp::Add { nodes, index: None }, ids)?;
                    out["mode"] = json!("shapes");
                    out["family"] = json!(family);
                    out["fallback"] = Value::Null;
                    return Ok(out);
                }
                Ok(_) => fallback = Some("the diagram plugin answered without a layout".to_owned()),
                Err(e) => fallback = Some(e),
            }
            if mode == "shapes" {
                return Err(ToolError {
                    code: "unavailable",
                    message: format!(
                        "this Mermaid cannot become shapes: {}",
                        fallback.unwrap_or_default()
                    ),
                });
            }
        }
        counter += 1;
        let block_id = crate::scene::format_node_id(counter);
        let mut block = ShapeNode::new(block_id.clone(), origin.x, origin.y, 360.0, 240.0);
        block.source = Some(input.source);
        let mut out = self.commit(
            id,
            &FigureOp::Add {
                nodes: vec![FigureNode::Mermaid(block)],
                index: None,
            },
            vec![block_id],
        )?;
        out["mode"] = json!("block");
        out["family"] = json!(family);
        out["fallback"] = json!(fallback);
        Ok(out)
    }

    fn export(
        &self,
        ctx: &Context<'_>,
        id: FigureId,
        scene: &FigureScene,
        a: &Value,
    ) -> Result<Value, ToolError> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            #[serde(default)]
            format: Option<String>,
            #[serde(default)]
            frame: Option<String>,
        }
        let input: Input = tools::parse(&tools::strip_target(a))?;
        let format = input.format.as_deref().unwrap_or("svg");
        if !matches!(format, "svg" | "pdf") {
            return Err(ToolError {
                code: "invalid_argument",
                message: format!("unknown format `{format}`; use svg or pdf"),
            });
        }
        let embeds = self.embeds_for(ctx, scene, Theme::Print);
        let saved = self.store.save(Some(id), None, &embeds)?;
        let pdf = if format == "pdf" || ctx.has_command("project.export-pdf") {
            pdf_for(ctx, &saved.svg, format == "pdf").map_err(|e| ToolError {
                code: "io",
                message: e.to_string(),
            })?
        } else {
            None
        };
        let frame_svg = match input.frame {
            Some(reference) => {
                let frame_id = tools::resolve_ref(scene, &reference)?;
                if !matches!(scene.node_by_id(&frame_id), Some(FigureNode::Frame(_))) {
                    return Err(ToolError {
                        code: "invalid_argument",
                        message: format!("`{frame_id}` is not a frame"),
                    });
                }
                let options = SvgExportOptions {
                    frame: Some(frame_id.clone()),
                    ..SvgExportOptions::default()
                };
                let svg = render_svg_with(scene, &options, &embeds);
                let (_, base_svg) = resolve_figure_paths(Path::new(&saved.json));
                let stem = base_svg
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("figure");
                let target = base_svg.with_file_name(format!("{stem}-{frame_id}.svg"));
                std::fs::write(&target, svg).map_err(|source| {
                    ToolError::from(FigureError::Io {
                        path: target.display().to_string(),
                        source,
                    })
                })?;
                Some(target.display().to_string())
            }
            None => None,
        };
        Ok(json!({
            "figure": self.figure_ref(id),
            "json": saved.json,
            "svg": saved.svg,
            "pdf": pdf,
            "frame_svg": frame_svg,
        }))
    }
}

const TOOL_COMMANDS: [&str; 10] = [
    "figure.describe",
    "figure.add-shapes",
    "figure.connect",
    "figure.update",
    "figure.remove",
    "figure.layout",
    "figure.from-mermaid",
    "figure.to-mermaid",
    "figure.style",
    "figure.export",
];

fn export_options(a: &Value) -> Result<SvgExportOptions, PluginError> {
    let mut options = SvgExportOptions::default();
    if let Some(t) = a.get("theme").and_then(Value::as_str) {
        options.theme = t.parse().map_err(PluginError::Failed)?;
    }
    if let Some(b) = a.get("background").and_then(Value::as_bool) {
        options.background = b;
    }
    if let Some(f) = a.get("frame").and_then(Value::as_str) {
        options.frame = Some(f.to_owned());
    }
    Ok(options)
}

impl Plugin for FigurePlugin {
    fn invoke(
        &self,
        ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let a = args(input)?;
        if TOOL_COMMANDS.contains(&command) {
            return self
                .tool(ctx, command, &a)
                .map(Payload::Json)
                .map_err(tool_failed);
        }
        match command {
            "figure.create" => {
                let width = a.get("width").and_then(Value::as_f64);
                let height = a.get("height").and_then(Value::as_f64);
                match a.get("path") {
                    None | Some(Value::Null) => {
                        let id = self.store.create(width, height);
                        Ok(Payload::Json(json!({ "id": id })))
                    }
                    Some(_) => {
                        let path = string(&a, "path")?;
                        let (id, saved) = self
                            .store
                            .create_at(Path::new(path), width, height)
                            .map_err(failed)?;
                        let pdf = pdf_for(ctx, &saved.svg, false)?;
                        Ok(Payload::Json(json!({
                            "id": id, "json": saved.json, "svg": saved.svg, "pdf": pdf,
                        })))
                    }
                }
            }
            "figure.open" => {
                let path = string(&a, "path")?;
                let id = self.store.open(Path::new(path)).map_err(failed)?;
                Ok(Payload::Json(json!({ "id": id })))
            }
            "figure.save" => {
                let fig_id = id(&a)?;
                let path = a.get("path").and_then(Value::as_str).map(Path::new);
                let scene = self.store.get(fig_id).map_err(failed)?;
                let embeds = self.embeds_for(ctx, &scene, Theme::Print);
                let saved = self.store.save(fig_id, path, &embeds).map_err(failed)?;
                let pdf = pdf_for(ctx, &saved.svg, false)?;
                let mut out = serde_json::to_value(saved).map_err(failed)?;
                if let Value::Object(map) = &mut out {
                    map.insert("pdf".to_owned(), pdf.map_or(Value::Null, Value::String));
                }
                Ok(Payload::Json(out))
            }
            "figure.close" => {
                let fig_id =
                    id(&a)?.ok_or_else(|| PluginError::Failed("`id` is required".to_owned()))?;
                let closed = self.store.close(fig_id).map_err(failed)?;
                Ok(Payload::Json(json!({ "closed": closed })))
            }
            "figure.list" => serde_json::to_value(self.store.list())
                .map(Payload::Json)
                .map_err(failed),
            "figure.apply" => {
                let fig_id = id(&a)?;
                let op_val = a
                    .get("op")
                    .ok_or_else(|| PluginError::Failed("missing `op` field".to_owned()))?;
                let op = FigureOp::from_value(op_val.clone()).map_err(failed)?;
                let res = self.store.apply(fig_id, &op).map_err(failed)?;
                serde_json::to_value(res).map(Payload::Json).map_err(failed)
            }
            "figure.get" => {
                let scene = self.store.get(id(&a)?).map_err(failed)?;
                serde_json::to_value(scene)
                    .map(Payload::Json)
                    .map_err(failed)
            }
            // Stateless: renders the scene the caller sends.
            "figure.render-svg" => {
                let scene_val = a
                    .get("scene")
                    .ok_or_else(|| PluginError::Failed("missing `scene` field".to_owned()))?;
                let scene = FigureScene::from_value(scene_val.clone())
                    .map_err(|e| PluginError::Failed(format!("invalid `scene`: {e}")))?;
                let options = export_options(&a)?;
                let embeds = self.embeds_for(ctx, &scene, options.theme);
                let svg = render_svg_with(&scene, &options, &embeds);
                Ok(Payload::Binary(Arc::from(svg.into_bytes())))
            }
            "figure.export-svg" => {
                let fig_id = id(&a)?;
                let options = export_options(&a)?;
                let scene = self.store.get(fig_id).map_err(failed)?;
                let embeds = self.embeds_for(ctx, &scene, options.theme);
                let svg = render_svg_with(&scene, &options, &embeds);
                Ok(Payload::Binary(Arc::from(svg.into_bytes())))
            }
            // Stateless: one formula to vector SVG (binary), for the canvas.
            "figure.render-math" => {
                let language: MathLanguage = match a.get("language").and_then(Value::as_str) {
                    None => MathLanguage::Typst,
                    Some(l) => l.parse().map_err(PluginError::Failed)?,
                };
                let source = string(&a, "source")?;
                let size = match a.get("size") {
                    None | Some(Value::Null) => 24.0,
                    Some(v) => v
                        .as_f64()
                        .ok_or_else(|| PluginError::Failed("`size` must be a number".to_owned()))?,
                };
                let rendered = self
                    .render_formula(ctx, language, source, size)
                    .map_err(|e| PluginError::Failed(e.to_tagged()))?;
                Ok(Payload::Binary(Arc::from(rendered.svg.into_bytes())))
            }
            "figure.hit-test" => {
                let fig_id = id(&a)?;
                let point = Vec2::new(number_f64(&a, "x")?, number_f64(&a, "y")?);
                let hit = self.store.hit_test(fig_id, point).map_err(failed)?;
                Ok(Payload::Json(match hit {
                    Some(node) => json!({ "hit": true, "node_id": node.id(), "node": node }),
                    None => json!({ "hit": false, "node_id": Value::Null, "node": Value::Null }),
                }))
            }
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }

    fn on_hook(
        &self,
        _ctx: &Context<'_>,
        hook: Hook,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        match hook {
            Hook::RegisterAgentTool => {
                let tools: Value = serde_json::from_str(tools::AGENT_TOOLS_JSON)
                    .map_err(|e| PluginError::Failed(format!("agent_tools.json: {e}")))?;
                Ok(Payload::Json(json!({ "tools": tools })))
            }
            _ => Ok(input),
        }
    }
}
