//! The kernel-facing command surface and event topics.
//!
//! Commands (JSON in, JSON out; no payload means `{}`):
//!
//! * `settings.get {id?}`: one setting `{id, value, source, default}`, or all
//!   `{values: {id: {value, source}}, warnings}`.
//! * `settings.set {id, value, scope?}`: `scope` is `user` (default) or
//!   `project`. Returns the new effective state.
//! * `settings.reset {id, scope?}`: remove the key from that layer's file.
//! * `settings.schema {}`: the full registry, for building a Settings UI.
//! * `settings.register {settings: [SettingDef], owner?}`: for plugins.
//! * `settings.open-file {scope}`: `user`, `project` or `keybindings`;
//!   creates the file if missing and returns `{path}`.
//! * `settings.use-project {path|null}`: select the project whose
//!   `.manang/settings.toml` forms the top layer.
//! * `keybindings.list|resolve|set|reset|register`, `theme.list|get`.
//!
//! Event topics on the bus:
//!
//! * `settings.changed {id, value, scope, requires-restart}`: the effective
//!   value of `id` changed; `scope` is the layer now supplying it (`default`,
//!   `user` or `project`). Fired for commands and for external file edits.
//! * `settings.registered {owner, ids}`
//! * `settings.warnings {warnings}`: the set of file problems changed.
//! * `settings.project-changed {path}`: `settings.use-project` swapped the
//!   project layer (`path` is the project root or `null`). Fired after the
//!   `settings.changed` events the swap caused.
//! * `keybindings.changed {}`
//!
//! Commands run synchronously and touch the disk, so the shell should invoke
//! them from a blocking task, not the UI thread.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{Context, EventBus, Manifest, ManifestError, Payload, Plugin, PluginError};
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use toml_edit::DocumentMut;

use crate::error::SettingsError;
use crate::keybindings::{self, KeybindingSet, When};
use crate::keys::{Platform, Shortcut};
use crate::layers::{self, Resolved, Source, Warning};
use crate::registry::{Registry, Scope};
use crate::store::{self, LayerFile};
use crate::theme::{Theme, ThemeStore};

const MANIFEST: &str = include_str!("../manifest.toml");
const KEYBINDINGS_MANIFEST: &str = include_str!("../keybindings.manifest.toml");
const THEME_MANIFEST: &str = include_str!("../theme.manifest.toml");

pub const SETTINGS_FILE: &str = "settings.toml";
pub const KEYBINDINGS_FILE: &str = "keybindings.toml";
pub const THEMES_DIR: &str = "themes";
/// Per-project state folder holding the project settings layer.
pub const PROJECT_DIR: &str = ".manang";

pub const TOPIC_CHANGED: &str = "settings.changed";
pub const TOPIC_REGISTERED: &str = "settings.registered";
pub const TOPIC_WARNINGS: &str = "settings.warnings";
pub const TOPIC_KEYBINDINGS: &str = "keybindings.changed";
/// `settings.use-project` selected (or cleared) the project layer.
pub const TOPIC_PROJECT: &str = "settings.project-changed";

/// Default watcher debounce window.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(100);

const USER_HEADER: &str = "# Manang user settings. Edit by hand; changes apply immediately.\n# See `settings.schema` (or the Settings screen) for every key.\n";
const PROJECT_HEADER: &str =
    "# Manang project settings. These override your user settings for this project only.\n";
const KEYS_HEADER: &str = "# Manang keybindings. Each [[bindings]] entry replaces that command's default keys.\n#\n# [[bindings]]\n# command = \"editor.save\"\n# keys = [\"Mod+S\"]\n# when = \"editorFocus\"\n";

/// The `settings` manifest (`settings.*` commands), parsed and validated.
pub fn manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// Manifest for the `keybindings.*` commands.
///
/// The kernel requires a command to be namespaced by its plugin's name, so
/// these live in their own manifest. Register the *same* plugin instance
/// under it (see [`manifests`]).
pub fn keybindings_manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(KEYBINDINGS_MANIFEST)
}

/// Manifest for the `theme.*` commands; see [`keybindings_manifest`].
pub fn theme_manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(THEME_MANIFEST)
}

/// All three manifests: `settings`, `keybindings`, `theme`. Register one
/// `Arc` of the plugin under each so every command is reachable.
pub fn manifests() -> Result<Vec<Manifest>, ManifestError> {
    Ok(vec![
        manifest()?,
        keybindings_manifest()?,
        theme_manifest()?,
    ])
}

/// Build the settings plugin.
///
/// * `config_dir`: the user config directory holding `settings.toml`,
///   `keybindings.toml` and `themes/`; the shell resolves the platform path.
/// * `events`: the bus change events are published on.
pub fn plugin(config_dir: impl Into<PathBuf>, events: EventBus) -> impl Plugin {
    SettingsPlugin::new(config_dir, events)
}

type Ev = (&'static str, Value);

struct State {
    registry: Registry,
    keys: KeybindingSet,
    user_file: PathBuf,
    keys_file: PathBuf,
    project_file: Option<PathBuf>,
    user: LayerFile,
    project: LayerFile,
    keys_warnings: Vec<String>,
    snapshot: BTreeMap<String, Resolved>,
    last_warnings: Value,
}

impl State {
    fn warnings(&self) -> Vec<Warning> {
        let mut out = Vec::new();
        for (layer, file) in [(Source::User, &self.user), (Source::Project, &self.project)] {
            if let Some(message) = &file.error {
                out.push(Warning {
                    layer,
                    id: None,
                    message: message.clone(),
                });
            }
            out.extend(layers::audit(&self.registry, layer, &file.values));
        }
        out
    }

    fn effective(&self) -> BTreeMap<String, Resolved> {
        layers::resolve_all(&self.registry, &self.user.values, &self.project.values)
    }

    /// Recompute effective values; emit `settings.changed` for differences
    /// and `settings.warnings` if the warning set moved.
    fn diff(&mut self) -> Vec<Ev> {
        let now = self.effective();
        let mut events = Vec::new();
        for (id, resolved) in &now {
            let Some(before) = self.snapshot.get(id) else {
                continue; // newly registered: nothing changed for listeners
            };
            if before != resolved {
                let restart = self.registry.get(id).is_some_and(|d| d.requires_restart);
                events.push((
                    TOPIC_CHANGED,
                    json!({
                        "id": id,
                        "value": resolved.value,
                        "scope": resolved.source.as_str(),
                        "requires-restart": restart,
                    }),
                ));
            }
        }
        self.snapshot = now;
        let warnings = serde_json::to_value(self.warnings()).unwrap_or(Value::Null);
        if warnings != self.last_warnings {
            events.push((TOPIC_WARNINGS, json!({ "warnings": warnings })));
            self.last_warnings = warnings;
        }
        events
    }

    fn reload_keys(&mut self) -> Vec<Ev> {
        let mut events = Vec::new();
        self.keys_warnings.clear();
        match store::read_text(&self.keys_file) {
            Ok(None) => {
                if !self.keys.user_entries().is_empty() {
                    self.keys.set_user(Vec::new());
                    events.push((TOPIC_KEYBINDINGS, json!({})));
                }
            }
            Ok(Some(text)) => match text.parse::<DocumentMut>() {
                Ok(doc) => {
                    let (entries, warnings) = keybindings::parse_user(&store::doc_to_json(&doc));
                    self.keys_warnings = warnings;
                    if entries != self.keys.user_entries() {
                        self.keys.set_user(entries);
                        events.push((TOPIC_KEYBINDINGS, json!({})));
                    }
                }
                // Keep the last good bindings while the file is mid-edit.
                Err(e) => self.keys_warnings.push(format!(
                    "{KEYBINDINGS_FILE} is not valid TOML ({}); keeping the previous bindings",
                    e.message()
                )),
            },
            Err(e) => self.keys_warnings.push(e.to_string()),
        }
        events
    }

    /// Re-read every file from disk.
    fn reload(&mut self) -> Vec<Ev> {
        self.user = store::read_layer(&self.user_file);
        self.project = self
            .project_file
            .as_deref()
            .map(store::read_layer)
            .unwrap_or_default();
        let mut events = self.diff();
        events.extend(self.reload_keys());
        events
    }

    fn project_path(&self) -> Result<&Path, SettingsError> {
        self.project_file.as_deref().ok_or(SettingsError::NoProject)
    }

    fn layer_path(&self, scope: Scope) -> Result<&Path, SettingsError> {
        match scope {
            Scope::User => Ok(&self.user_file),
            Scope::Project => self.project_path(),
        }
    }

    fn describe(&self, id: &str) -> Result<Value, SettingsError> {
        let def = self
            .registry
            .get(id)
            .ok_or_else(|| SettingsError::UnknownSetting(id.to_owned()))?;
        let resolved = layers::resolve(def, &self.user.values, &self.project.values);
        Ok(json!({
            "id": id,
            "value": resolved.value,
            "source": resolved.source,
            "default": def.default,
            "requires-restart": def.requires_restart,
        }))
    }
}

struct Shared {
    state: Mutex<State>,
    events: EventBus,
    themes: ThemeStore,
    config_dir: PathBuf,
}

impl Shared {
    fn emit(&self, events: Vec<Ev>) {
        for (topic, payload) in events {
            self.events.emit(topic, Payload::Json(payload));
        }
    }

    /// Run `f` under the state lock, then publish its events unlocked.
    fn with<T>(
        &self,
        f: impl FnOnce(&mut State) -> Result<(T, Vec<Ev>), SettingsError>,
    ) -> Result<T, SettingsError> {
        let (out, events) = {
            let mut state = self.state.lock();
            f(&mut state)?
        };
        self.emit(events);
        Ok(out)
    }

    fn reload(&self) {
        let events = self.state.lock().reload();
        self.emit(events);
    }
}

/// Implements [`Plugin`] for the settings, keybinding and theme commands.
pub struct SettingsPlugin {
    shared: Arc<Shared>,
    debounce: Duration,
    watcher: Mutex<Option<Debouncer<RecommendedWatcher, RecommendedCache>>>,
    watch_error: Mutex<Option<String>>,
}

impl SettingsPlugin {
    pub fn new(config_dir: impl Into<PathBuf>, events: EventBus) -> Self {
        Self::with_platform(config_dir, events, Platform::current())
    }

    /// As [`SettingsPlugin::new`] with an explicit platform (for tests).
    pub fn with_platform(
        config_dir: impl Into<PathBuf>,
        events: EventBus,
        platform: Platform,
    ) -> Self {
        let config_dir = config_dir.into();
        let registry = Registry::with_core();
        let mut state = State {
            snapshot: BTreeMap::new(),
            registry,
            keys: KeybindingSet::with_core(platform),
            user_file: config_dir.join(SETTINGS_FILE),
            keys_file: config_dir.join(KEYBINDINGS_FILE),
            project_file: None,
            user: LayerFile::default(),
            project: LayerFile::default(),
            keys_warnings: Vec::new(),
            last_warnings: Value::Null,
        };
        state.user = store::read_layer(&state.user_file);
        state.snapshot = state.effective();
        state.last_warnings = serde_json::to_value(state.warnings()).unwrap_or(Value::Null);
        let _ = state.reload_keys();
        let plugin = Self {
            shared: Arc::new(Shared {
                state: Mutex::new(state),
                events,
                themes: ThemeStore::new(config_dir.join(THEMES_DIR)),
                config_dir,
            }),
            debounce: DEFAULT_DEBOUNCE,
            watcher: Mutex::new(None),
            watch_error: Mutex::new(None),
        };
        plugin.restart_watch();
        plugin
    }

    /// Override the file-watch debounce window (default 100 ms).
    #[must_use]
    pub fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce = debounce;
        self.restart_watch();
        self
    }

    /// Why file watching is off, if it is.
    pub fn watch_error(&self) -> Option<String> {
        self.watch_error.lock().clone()
    }

    /// Re-read every file now (what the watcher does on external edits).
    pub fn reload(&self) {
        self.shared.reload();
    }

    fn restart_watch(&self) {
        let mut dirs = vec![self.shared.config_dir.clone()];
        if let Some(project_file) = self.shared.state.lock().project_file.clone()
            && let Some(dir) = project_file.parent()
        {
            dirs.push(dir.to_path_buf());
            if let Some(root) = dir.parent() {
                dirs.push(root.to_path_buf());
            }
        }
        // Dropping the old watcher stops it.
        *self.watcher.lock() = None;
        match start_watch(&dirs, &self.shared, self.debounce) {
            Ok(w) => {
                *self.watcher.lock() = Some(w);
                *self.watch_error.lock() = None;
            }
            Err(e) => *self.watch_error.lock() = Some(e.to_string()),
        }
    }
}

fn start_watch(
    dirs: &[PathBuf],
    shared: &Arc<Shared>,
    debounce: Duration,
) -> Result<Debouncer<RecommendedWatcher, RecommendedCache>, SettingsError> {
    // The config dir may not exist on first run; watching needs it to.
    for dir in dirs.iter().take(1) {
        std::fs::create_dir_all(dir)
            .map_err(|e| SettingsError::io("creating the config folder", e))?;
    }
    let handler_shared = Arc::clone(shared);
    let mut debouncer = new_debouncer(debounce, None, move |result: DebounceEventResult| {
        let Ok(events) = result else { return };
        let relevant = events.iter().any(|e| {
            e.event.paths.iter().any(|p| {
                p.file_name()
                    .is_some_and(|n| n == SETTINGS_FILE || n == KEYBINDINGS_FILE)
            })
        });
        if relevant {
            handler_shared.reload();
        }
    })
    .map_err(|e| SettingsError::Watch(e.to_string()))?;
    let mut watched = 0;
    let mut last_error = None;
    for dir in dirs {
        match debouncer.watch(dir, RecursiveMode::NonRecursive) {
            Ok(()) => watched += 1,
            Err(e) => last_error = Some(e.to_string()),
        }
    }
    match (watched, last_error) {
        (0, Some(e)) => Err(SettingsError::Watch(e)),
        _ => Ok(debouncer),
    }
}

fn failed(e: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(e.to_string())
}

fn parse<T: DeserializeOwned>(input: Payload) -> Result<T, PluginError> {
    let value = match input {
        Payload::Json(Value::Null) | Payload::Empty => json!({}),
        Payload::Json(v) => v,
        Payload::Binary(_) => return Err(failed("expected a JSON payload")),
    };
    serde_json::from_value(value).map_err(|e| failed(format!("bad arguments: {e}")))
}

fn json_out<T: serde::Serialize>(value: &T) -> Result<Payload, PluginError> {
    serde_json::to_value(value)
        .map(Payload::Json)
        .map_err(failed)
}

mod args {
    use serde::Deserialize;
    use serde_json::Value;

    use crate::keybindings::DefaultSpec;
    use crate::registry::{Scope, SettingDef};

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Get {
        pub id: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Set {
        pub id: String,
        pub value: Value,
        pub scope: Option<Scope>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Reset {
        pub id: String,
        pub scope: Option<Scope>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Register {
        pub settings: Vec<SettingDef>,
        pub owner: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct OpenFile {
        pub scope: FileScope,
    }

    #[derive(Deserialize, Clone, Copy)]
    #[serde(rename_all = "lowercase")]
    pub enum FileScope {
        User,
        Project,
        Keybindings,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct UseProject {
        pub path: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Resolve {
        #[serde(default)]
        pub context: Value,
        pub key: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KeySet {
        pub command: String,
        pub keys: Option<Vec<String>>,
        pub key: Option<String>,
        pub when: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KeyReset {
        pub command: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KeyRegister {
        pub bindings: Vec<DefaultSpec>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ThemeGet {
        pub id: String,
    }
}

fn contexts(raw: &Value) -> Result<HashSet<String>, PluginError> {
    match raw {
        Value::Null => Ok(HashSet::new()),
        Value::String(s) => Ok(s
            .split([',', ' '])
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect()),
        Value::Array(items) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| failed("`context` must be a string or an array of strings"))
            })
            .collect(),
        _ => Err(failed("`context` must be a string or an array of strings")),
    }
}

fn valid_owner(owner: &str) -> bool {
    let mut chars = owner.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && owner != "core"
}

impl SettingsPlugin {
    fn get(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::Get = parse(input)?;
        let state = self.shared.state.lock();
        match a.id {
            Some(id) => state.describe(&id).map_err(failed).map(Payload::Json),
            None => {
                let values: serde_json::Map<String, Value> = state
                    .effective()
                    .into_iter()
                    .map(|(id, r)| (id, json!({"value": r.value, "source": r.source})))
                    .collect();
                Ok(Payload::Json(json!({
                    "values": values,
                    "warnings": state.warnings(),
                })))
            }
        }
    }

    fn set(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::Set = parse(input)?;
        let scope = a.scope.unwrap_or(Scope::User);
        let out = self.shared.with(|state| {
            let def = state
                .registry
                .get(&a.id)
                .ok_or_else(|| SettingsError::UnknownSetting(a.id.clone()))?;
            if scope == Scope::Project && def.scope != Scope::Project {
                return Err(SettingsError::NotProjectScoped { id: a.id.clone() });
            }
            let value = def.validate(&a.value)?;
            let path = state.layer_path(scope)?.to_path_buf();
            store::edit_file(&path, |doc| store::set_value(doc, &a.id, &value))?;
            reload_layer(state, scope);
            let events = state.diff();
            let mut described = state.describe(&a.id)?;
            described["written"] = json!(scope.as_str());
            Ok((described, events))
        });
        if scope == Scope::Project {
            // Creating `.manang/` may have added a directory worth watching.
            self.restart_watch();
        }
        out.map(Payload::Json).map_err(failed)
    }

    fn reset(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::Reset = parse(input)?;
        let scope = a.scope.unwrap_or(Scope::User);
        self.shared
            .with(|state| {
                let path = state.layer_path(scope)?.to_path_buf();
                let exists = path.exists();
                if exists {
                    store::edit_file(&path, |doc| Ok(store::remove_value(doc, &a.id)))?;
                    reload_layer(state, scope);
                }
                let events = state.diff();
                let described = state
                    .describe(&a.id)
                    .unwrap_or_else(|_| json!({"id": a.id}));
                Ok((described, events))
            })
            .map(Payload::Json)
            .map_err(failed)
    }

    fn register(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::Register = parse(input)?;
        let owner = a.owner.unwrap_or_else(|| "external".to_owned());
        if !valid_owner(&owner) {
            return Err(failed(format!(
                "owner `{owner}` must be lowercase kebab-case and not `core`"
            )));
        }
        let ids: Vec<String> = a.settings.iter().map(|d| d.id.clone()).collect();
        self.shared
            .with(|state| {
                let added = state.registry.register_all(a.settings, &owner)?;
                let fresh = state.effective();
                for (id, resolved) in fresh {
                    state.snapshot.entry(id).or_insert(resolved);
                }
                let mut events = Vec::new();
                if added > 0 {
                    events.push((TOPIC_REGISTERED, json!({"owner": owner, "ids": ids})));
                }
                let warnings: Vec<Warning> = state
                    .warnings()
                    .into_iter()
                    .filter(|w| w.id.as_ref().is_some_and(|id| ids.contains(id)))
                    .collect();
                events.extend(state.diff());
                Ok((json!({"registered": added, "warnings": warnings}), events))
            })
            .map(Payload::Json)
            .map_err(failed)
    }

    fn open_file(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::OpenFile = parse(input)?;
        let out = self.shared.with(|state| {
            let (path, header) = match a.scope {
                args::FileScope::User => (state.user_file.clone(), USER_HEADER),
                args::FileScope::Project => (state.project_path()?.to_path_buf(), PROJECT_HEADER),
                args::FileScope::Keybindings => (state.keys_file.clone(), KEYS_HEADER),
            };
            let created = !path.exists();
            if created {
                store::atomic_write(&path, header.as_bytes())?;
            }
            Ok((
                json!({"path": path.to_string_lossy(), "created": created}),
                Vec::new(),
            ))
        });
        if matches!(a.scope, args::FileScope::Project) {
            self.restart_watch();
        }
        out.map(Payload::Json).map_err(failed)
    }

    fn use_project(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::UseProject = parse(input)?;
        let out = self.shared.with(|state| {
            state.project_file = match &a.path {
                None => None,
                Some(p) => {
                    let root = Path::new(p);
                    if !root.is_dir() {
                        return Err(SettingsError::io(
                            format!("selecting project {p}"),
                            std::io::Error::new(std::io::ErrorKind::NotFound, "not a directory"),
                        ));
                    }
                    Some(root.join(PROJECT_DIR).join(SETTINGS_FILE))
                }
            };
            state.project = state
                .project_file
                .as_deref()
                .map(store::read_layer)
                .unwrap_or_default();
            let mut events = state.diff();
            events.push((TOPIC_PROJECT, json!({"path": a.path})));
            Ok((json!({"warnings": state.warnings()}), events))
        });
        self.restart_watch();
        out.map(Payload::Json).map_err(failed)
    }

    fn key_list(&self) -> Result<Payload, PluginError> {
        let state = self.shared.state.lock();
        let mut out = state.keys.list_json();
        out["warnings"] = json!(state.keys_warnings);
        Ok(Payload::Json(out))
    }

    fn key_resolve(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::Resolve = parse(input)?;
        let ctx = contexts(&a.context)?;
        let resolution = self
            .shared
            .state
            .lock()
            .keys
            .resolve(&ctx, &a.key)
            .map_err(failed)?;
        json_out(&resolution)
    }

    fn key_set(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::KeySet = parse(input)?;
        self.shared
            .with(|state| {
                keybindings::valid_command(&a.command)?;
                let raw: Vec<String> = match (a.keys, a.key) {
                    (Some(keys), None) => keys,
                    (None, Some(key)) => vec![key],
                    (None, None) => Vec::new(),
                    (Some(_), Some(_)) => {
                        return Err(SettingsError::Unstorable {
                            path: a.command.clone(),
                            message: "give either `key` or `keys`, not both".to_owned(),
                        });
                    }
                };
                let keys: Vec<Shortcut> = raw
                    .iter()
                    .map(|k| Shortcut::parse(k))
                    .collect::<Result<_, _>>()?;
                let when = When::parse(a.when.as_deref().unwrap_or(""))?;
                let path = state.keys_file.clone();
                let shown = path.display().to_string();
                store::edit_file(&path, |doc| {
                    keybindings::write_override(doc, &shown, &a.command, &keys, &when)
                })?;
                let events = state.reload_keys();
                let conflicts: Vec<_> = state
                    .keys
                    .conflicts()
                    .into_iter()
                    .filter(|c| c.commands.contains(&a.command))
                    .collect();
                let stored: Vec<String> = keys.iter().map(ToString::to_string).collect();
                Ok((
                    json!({"command": a.command, "keys": stored, "when": when, "conflicts": conflicts}),
                    events,
                ))
            })
            .map(Payload::Json)
            .map_err(failed)
    }

    fn key_reset(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::KeyReset = parse(input)?;
        self.shared
            .with(|state| {
                let path = state.keys_file.clone();
                let shown = path.display().to_string();
                let removed = if path.exists() {
                    store::edit_file(&path, |doc| {
                        keybindings::remove_override(doc, &shown, a.command.as_deref())
                    })?;
                    true
                } else {
                    false
                };
                let events = if removed {
                    state.reload_keys()
                } else {
                    Vec::new()
                };
                Ok((json!({"command": a.command}), events))
            })
            .map(Payload::Json)
            .map_err(failed)
    }

    fn key_register(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::KeyRegister = parse(input)?;
        self.shared
            .with(|state| {
                let added = state.keys.register(a.bindings)?;
                let events = if added > 0 {
                    vec![(TOPIC_KEYBINDINGS, json!({}))]
                } else {
                    Vec::new()
                };
                Ok((json!({"registered": added}), events))
            })
            .map(Payload::Json)
            .map_err(failed)
    }

    fn theme_list(&self) -> Result<Payload, PluginError> {
        let (themes, warnings) = self.shared.themes.list();
        let summary: Vec<Value> = themes
            .iter()
            .map(|t: &Theme| {
                json!({
                    "id": t.id, "name": t.name, "scheme": t.scheme,
                    "builtin": t.builtin, "base": t.base,
                })
            })
            .collect();
        Ok(Payload::Json(
            json!({"themes": summary, "warnings": warnings}),
        ))
    }

    fn theme_get(&self, input: Payload) -> Result<Payload, PluginError> {
        let a: args::ThemeGet = parse(input)?;
        let theme = self.shared.themes.get(&a.id).map_err(failed)?;
        json_out(&theme)
    }
}

/// Re-read one layer's file into state.
fn reload_layer(state: &mut State, scope: Scope) {
    match scope {
        Scope::User => state.user = store::read_layer(&state.user_file),
        Scope::Project => {
            state.project = state
                .project_file
                .as_deref()
                .map(store::read_layer)
                .unwrap_or_default();
        }
    }
}

impl Plugin for SettingsPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        match command {
            "settings.get" => self.get(input),
            "settings.set" => self.set(input),
            "settings.reset" => self.reset(input),
            "settings.schema" => Ok(Payload::Json(self.shared.state.lock().registry.schema())),
            "settings.register" => self.register(input),
            "settings.open-file" => self.open_file(input),
            "settings.use-project" => self.use_project(input),
            "keybindings.list" => self.key_list(),
            "keybindings.resolve" => self.key_resolve(input),
            "keybindings.set" => self.key_set(input),
            "keybindings.reset" => self.key_reset(input),
            "keybindings.register" => self.key_register(input),
            "theme.list" => self.theme_list(),
            "theme.get" => self.theme_get(input),
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }
}
