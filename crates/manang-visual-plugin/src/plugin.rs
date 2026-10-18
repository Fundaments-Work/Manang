use std::sync::Arc;

use manang_editor::Editor;
use manang_kernel::{Context, Payload, Plugin, PluginError};
use manang_visual::{Change, FormatAdapter, SessionHost, VisualError};
use rayon::{ThreadPool, ThreadPoolBuilder};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use thiserror::Error;

/// Building the plugin failed.
#[derive(Debug, Error)]
pub enum VisualHostError {
    #[error("could not start the visual worker pool: {0}")]
    Pool(String),
    #[error(transparent)]
    Adapter(#[from] VisualError),
}

/// Toolbar commands: `visual.<name>` runs the adapter command `<name>`.
const TOOLBAR_COMMANDS: &[&str] = &[
    "bold", "italic", "code", "strike", "heading", "list", "quote", "link", "image", "figure",
    "math", "rule",
];

/// The visual plugin.
pub struct VisualPlugin {
    editor: Arc<Editor>,
    host: SessionHost,
    pool: ThreadPool,
}

impl VisualPlugin {
    /// A plugin reading document text from `editor`. Add formats with
    /// [`with_adapter`](Self::with_adapter).
    pub fn new(editor: Arc<Editor>) -> Result<Self, VisualHostError> {
        let threads = std::thread::available_parallelism()
            .map_or(2, std::num::NonZero::get)
            .clamp(1, 4);
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("manang-visual-{i}"))
            .build()
            .map_err(|e| VisualHostError::Pool(e.to_string()))?;
        Ok(Self {
            editor,
            host: SessionHost::new(),
            pool,
        })
    }

    /// Register a format adapter.
    pub fn with_adapter(self, adapter: Arc<dyn FormatAdapter>) -> Result<Self, VisualHostError> {
        self.host.register(adapter)?;
        Ok(self)
    }

    /// The session host, to register adapters of plugins loaded later.
    pub fn host(&self) -> &SessionHost {
        &self.host
    }
}

fn coded(e: &VisualError) -> PluginError {
    PluginError::Failed(format!("[{}] {e}", e.code()))
}

fn bad_args(message: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(format!("[visual.bad-args] {message}"))
}

fn parse<T: DeserializeOwned>(input: Payload) -> Result<T, PluginError> {
    let value = match input {
        Payload::Json(Value::Null) | Payload::Empty => json!({}),
        Payload::Json(v) => v,
        Payload::Binary(_) => return Err(bad_args("expected a JSON payload")),
    };
    serde_json::from_value(value).map_err(bad_args)
}

fn json_out<T: serde::Serialize>(value: &T) -> Result<Payload, PluginError> {
    serde_json::to_value(value)
        .map(Payload::Json)
        .map_err(|e| PluginError::Failed(format!("[visual.failed] {e}")))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenArgs {
    doc: Option<u64>,
    text: Option<String>,
    adapter: Option<String>,
    path: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResetArgs {
    session: u32,
    doc: Option<u64>,
    text: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionArgs {
    session: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditArgs {
    session: u32,
    changes: Vec<Change>,
    len: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RangeArgs {
    session: u32,
    from: usize,
    to: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunArgs {
    session: u32,
    command: String,
    from: usize,
    to: usize,
    #[serde(default)]
    args: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolbarArgs {
    session: u32,
    from: usize,
    to: usize,
    #[serde(default)]
    args: String,
}

impl VisualPlugin {
    /// The text of `doc` or the inline `text`, never both.
    fn source(&self, doc: Option<u64>, text: Option<String>) -> Result<String, PluginError> {
        match (doc, text) {
            (Some(_), Some(_)) => Err(bad_args("give `doc` or `text`, not both")),
            (None, Some(text)) => Ok(text),
            (Some(id), None) => self
                .editor
                .text(Some(id))
                .map_err(|e| bad_args(format!("document {id}: {e}"))),
            (None, None) => Err(bad_args("`doc` or `text` is required")),
        }
    }

    fn run_command(&self, a: &RunArgs) -> Result<Payload, PluginError> {
        let plan = self
            .host
            .with(a.session, |s| {
                s.run_command(&a.command, a.from, a.to, &a.args)
            })
            .map_err(|e| coded(&e))?;
        json_out(&plan)
    }
}

impl Plugin for VisualPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        // Parsing and conversion are CPU work: keep them off the caller's
        // (Tokio) thread.
        self.pool.install(|| self.dispatch(command, input))
    }
}

impl VisualPlugin {
    fn dispatch(&self, command: &str, input: Payload) -> Result<Payload, PluginError> {
        match command {
            "visual.adapters" => {
                parse::<NoArgs>(input)?;
                json_out(&self.host.adapters())
            }
            "visual.open" => {
                let a: OpenArgs = parse(input)?;
                // A document opened from the editor tells its adapter by its path.
                let path = match (&a.path, a.doc) {
                    (Some(p), _) => Some(p.clone()),
                    (None, Some(id)) => self.editor.info(Some(id)).ok().and_then(|i| i.path),
                    (None, None) => None,
                };
                let text = self.source(a.doc, a.text)?;
                let info = self
                    .host
                    .open(&text, a.adapter.as_deref(), path.as_deref())
                    .map_err(|e| coded(&e))?;
                json_out(&info)
            }
            "visual.reset" => {
                let a: ResetArgs = parse(input)?;
                let text = self.source(a.doc, a.text)?;
                let (revision, len) = self
                    .host
                    .with(a.session, |s| {
                        s.reset(&text)?;
                        Ok((s.revision(), s.len()))
                    })
                    .map_err(|e| coded(&e))?;
                Ok(Payload::Json(
                    json!({ "session": a.session, "revision": revision, "len": len }),
                ))
            }
            "visual.close" => {
                let a: SessionArgs = parse(input)?;
                self.host.close(a.session);
                Ok(Payload::Empty)
            }
            "visual.edit" => {
                let a: EditArgs = parse(input)?;
                let out = self
                    .host
                    .with(a.session, |s| s.edit(&a.changes, a.len))
                    .map_err(|e| coded(&e))?;
                json_out(&out)
            }
            "visual.blocks" => {
                let a: RangeArgs = parse(input)?;
                let table = self
                    .host
                    .with(a.session, |s| s.blocks(a.from, a.to))
                    .map_err(|e| coded(&e))?;
                Ok(Payload::Binary(Arc::from(table.encode())))
            }
            "visual.outline" => {
                let a: SessionArgs = parse(input)?;
                let out = self
                    .host
                    .with(a.session, |s| Ok(s.outline()))
                    .map_err(|e| coded(&e))?;
                json_out(&out)
            }
            "visual.command-state" => {
                let a: RangeArgs = parse(input)?;
                let out = self
                    .host
                    .with(a.session, |s| Ok(s.command_state(a.from, a.to)))
                    .map_err(|e| coded(&e))?;
                json_out(&out)
            }
            "visual.run-command" => self.run_command(&parse::<RunArgs>(input)?),
            other => match other
                .strip_prefix("visual.")
                .filter(|name| TOOLBAR_COMMANDS.contains(name))
            {
                Some(name) => {
                    let a: ToolbarArgs = parse(input)?;
                    self.run_command(&RunArgs {
                        session: a.session,
                        command: name.to_owned(),
                        from: a.from,
                        to: a.to,
                        args: a.args,
                    })
                }
                None => Err(PluginError::UnknownCommand(other.to_owned())),
            },
        }
    }
}
