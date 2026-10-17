//! The kernel-facing command surface.
//!
//! One project is open at a time. `project.open` (or `project.create`) makes it
//! current, starts the file watcher, and records it in the recent list; every
//! other command acts on the current project and fails with "no project is
//! open" otherwise.
//!
//! Commands run synchronously on the caller's thread: the shell should invoke
//! them from a blocking task, not the UI thread.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{Context, EventBus, Manifest, ManifestError, Payload, Plugin, PluginError};
use parking_lot::RwLock;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::config::{CONFIG_FILE, ProjectConfig};
use crate::error::ProjectError;
use crate::fsops;
use crate::git;
use crate::paths::ProjectRoot;
use crate::recent::RecentStore;
use crate::scaffold::{self, NewProject};
use crate::tree::{self, TreeQuery};
use crate::watch::{DEFAULT_DEBOUNCE, ProjectWatcher};

const MANIFEST: &str = include_str!("../manifest.toml");

/// The plugin's manifest, parsed and validated.
pub fn manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Build the project plugin.
///
/// * `config_dir`: the user config directory (where `recent-projects.toml`
///   lives); the shell resolves the platform path.
/// * `events`: the bus change events are published on; see
///   [`crate::watch`] for the topics.
pub fn plugin(config_dir: impl Into<PathBuf>, events: EventBus) -> impl Plugin {
    ProjectPlugin::new(config_dir, events)
}

struct Open {
    root: ProjectRoot,
    config: ProjectConfig,
    _watcher: Option<ProjectWatcher>,
}

/// Implements [`Plugin`] for the `project.*` commands.
pub struct ProjectPlugin {
    state: RwLock<Option<Arc<Open>>>,
    recent: RecentStore,
    events: EventBus,
    debounce: Duration,
}

impl ProjectPlugin {
    pub fn new(config_dir: impl Into<PathBuf>, events: EventBus) -> Self {
        Self {
            state: RwLock::new(None),
            recent: RecentStore::new(config_dir.into()),
            events,
            debounce: DEFAULT_DEBOUNCE,
        }
    }

    /// Override the watcher debounce window (default 150 ms).
    #[must_use]
    pub fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce = debounce;
        self
    }

    fn current(&self) -> Result<Arc<Open>, ProjectError> {
        self.state.read().clone().ok_or(ProjectError::NoProject)
    }

    fn open(&self, dir: &Path) -> Result<Value, ProjectError> {
        let root = ProjectRoot::open(dir)?;
        let config = read_config(&root)?;
        let mut warnings: Vec<String> = Vec::new();

        let watcher = match ProjectWatcher::start(root.path(), self.events.clone(), self.debounce) {
            Ok(w) => Some(w),
            Err(e) => {
                warnings.push(format!("file watching is unavailable: {e}"));
                None
            }
        };
        let name = display_name(&root, &config);
        if let Err(e) = self.recent.touch(root.path(), &name) {
            warnings.push(format!("could not update the recent projects list: {e}"));
        }
        if !root.path().join(config.main_file()).is_file() {
            warnings.push(format!(
                "main file `{}` does not exist yet",
                config.main_file()
            ));
        }

        let page = tree::list(&root, &TreeQuery::default())?;
        let summary = summarize(&root, &config, &name, &warnings, &page)?;
        // Replacing the state drops the previous watcher.
        *self.state.write() = Some(Arc::new(Open {
            root,
            config,
            _watcher: watcher,
        }));
        Ok(summary)
    }
}

fn display_name(root: &ProjectRoot, config: &ProjectConfig) -> String {
    config.project.name.clone().unwrap_or_else(|| {
        root.path().file_name().map_or_else(
            || "project".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    })
}

fn read_config(root: &ProjectRoot) -> Result<ProjectConfig, ProjectError> {
    let file = root.path().join(CONFIG_FILE);
    let text = match fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProjectError::NotAProject(root.path().display().to_string()));
        }
        Err(e) => return Err(ProjectError::io(format!("reading {CONFIG_FILE}"), e)),
    };
    Ok(ProjectConfig::parse(&text)?)
}

fn summarize(
    root: &ProjectRoot,
    config: &ProjectConfig,
    name: &str,
    warnings: &[String],
    page: &tree::TreePage,
) -> Result<Value, ProjectError> {
    Ok(json!({
        "root": root.path().to_string_lossy(),
        "name": name,
        "engine": config.project.engine,
        "main": config.main_file(),
        "config": to_json(config)?,
        "warnings": warnings,
        "tree": to_json(page)?,
    }))
}

fn to_json<T: Serialize>(value: &T) -> Result<Value, ProjectError> {
    serde_json::to_value(value)
        .map_err(|e| ProjectError::io("serialising a response", std::io::Error::other(e)))
}

fn failed(e: ProjectError) -> PluginError {
    PluginError::Failed(e.to_string())
}

/// Decode command arguments; no payload means `{}`. Unknown fields are
/// rejected by the argument structs.
fn parse<T: DeserializeOwned>(input: Payload) -> Result<T, PluginError> {
    let value = match input {
        Payload::Json(Value::Null) | Payload::Empty => json!({}),
        Payload::Json(v) => v,
        Payload::Binary(_) => {
            return Err(PluginError::Failed("expected a JSON payload".to_owned()));
        }
    };
    serde_json::from_value(value).map_err(|e| PluginError::Failed(format!("bad arguments: {e}")))
}

fn json_payload<T: Serialize>(value: &T) -> Result<Payload, PluginError> {
    to_json(value).map(Payload::Json).map_err(failed)
}

mod args {
    use serde::Deserialize;

    use crate::config::Engine;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Dir {
        pub path: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Create {
        pub path: String,
        pub name: String,
        pub engine: Engine,
        #[serde(default)]
        pub template: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct File {
        pub path: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Write {
        pub path: String,
        pub content: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Rename {
        pub from: String,
        pub to: String,
    }
}

impl Plugin for ProjectPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        match command {
            "project.open" => {
                let a: args::Dir = parse(input)?;
                let summary = self.open(Path::new(&a.path)).map_err(failed)?;
                Ok(Payload::Json(summary))
            }
            "project.create" => {
                let a: args::Create = parse(input)?;
                let dir = Path::new(&a.path);
                scaffold::create(
                    dir,
                    &NewProject {
                        name: &a.name,
                        engine: a.engine,
                        template: a.template.as_deref(),
                    },
                )
                .map_err(failed)?;
                let summary = self.open(dir).map_err(failed)?;
                Ok(Payload::Json(summary))
            }
            "project.close" => {
                *self.state.write() = None;
                Ok(Payload::Empty)
            }
            "project.config" => {
                let open = self.current().map_err(failed)?;
                json_payload(&open.config)
            }
            "project.tree" => {
                let query: TreeQuery = parse(input)?;
                let open = self.current().map_err(failed)?;
                json_payload(&tree::list(&open.root, &query).map_err(failed)?)
            }
            "project.read-file" => {
                let a: args::File = parse(input)?;
                let open = self.current().map_err(failed)?;
                let text = fsops::read_text(&open.root, &a.path).map_err(failed)?;
                Ok(Payload::Json(Value::String(text)))
            }
            "project.read-bytes" => {
                let a: args::File = parse(input)?;
                let open = self.current().map_err(failed)?;
                let bytes = fsops::read_bytes(&open.root, &a.path).map_err(failed)?;
                Ok(Payload::Binary(bytes.into()))
            }
            "project.write-file" => {
                let a: args::Write = parse(input)?;
                let open = self.current().map_err(failed)?;
                let bytes = fsops::write_text(&open.root, &a.path, &a.content).map_err(failed)?;
                Ok(Payload::Json(json!({ "bytes": bytes })))
            }
            "project.rename" => {
                let a: args::Rename = parse(input)?;
                let open = self.current().map_err(failed)?;
                fsops::rename(&open.root, &a.from, &a.to).map_err(failed)?;
                Ok(Payload::Empty)
            }
            "project.delete" => {
                let a: args::File = parse(input)?;
                let open = self.current().map_err(failed)?;
                let trashed = fsops::trash(&open.root, &a.path).map_err(failed)?;
                Ok(Payload::Json(json!({ "trashed_to": trashed })))
            }
            "project.recent" => {
                let entries: Vec<Value> = self
                    .recent
                    .list()
                    .into_iter()
                    .map(|e| {
                        json!({
                            "exists": Path::new(&e.path).join(CONFIG_FILE).is_file(),
                            "path": e.path,
                            "name": e.name,
                            "opened_at": e.opened_at,
                        })
                    })
                    .collect();
                Ok(Payload::Json(Value::Array(entries)))
            }
            "project.git-status" => {
                let open = self.current().map_err(failed)?;
                json_payload(&git::status(open.root.path()).map_err(failed)?)
            }
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }
}
