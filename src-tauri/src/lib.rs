//! Manang desktop shell.
//!
//! AGENTS.md: all compute-heavy work lives in the Rust core, and the frontend
//! is a thin presentation layer. This crate owns only the IPC surface and the
//! application lifecycle; the document engine arrives as sibling crates in
//! `crates/`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use manang_editor::{Editor, EditorPlugin};
use manang_kernel::{BufferService, CommandInfo, EventBus, Kernel, KernelError, Payload};
use manang_plugins::{ManagerConfig, PluginEvent, PluginManager, WatchHandle, WatchOptions};
use manang_settings::PluginSettingsSync;
use manang_wasm_host::{HostServices, ProjectRoot};
use serde::Serialize;
use serde_json::Value;
use tauri::{Emitter, State};

/// Build identity, surfaced to the shell for the about dialogue.
///
/// Returned as plain data rather than a greeting string so the frontend never
/// parses strings out of the IPC boundary.
#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub target: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Manang",
        version: env!("CARGO_PKG_VERSION"),
        target: "typst",
    }
}

/// Structured failure for the shell; it never parses error strings.
#[derive(Debug, Serialize)]
pub struct IpcError {
    pub message: String,
}

impl From<KernelError> for IpcError {
    fn from(error: KernelError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

/// Why the shell could not start.
#[derive(Debug, thiserror::Error)]
enum BootError {
    #[error(transparent)]
    Kernel(#[from] KernelError),
    #[error(transparent)]
    Typst(#[from] manang_typst::TypstError),
    #[error(transparent)]
    Manifest(#[from] manang_kernel::ManifestError),
    #[error(transparent)]
    Plugins(#[from] manang_plugins::ManagerError),
    #[error(transparent)]
    Settings(#[from] manang_settings::SyncError),
}

/// Directories the shell hands to plugins.
struct Dirs<'a> {
    config: &'a std::path::Path,
    cache: &'a std::path::Path,
}

/// The default LaTeX backend is the system TeX installation. The optional
/// built-in Tectonic engine runs in a separate worker process, never linked into
/// this binary (static HarfBuzz crashes GTK/WebKit, AGENTS.md 7.4).
///
/// For local testing, a debug build uses the worker that
/// `cargo build -p manang-latex --features engine --bin manang-latex-worker`
/// leaves next to this executable. It is not bundled with release builds.
/// `MANANG_LATEX_WORKER` overrides. `None` lets the plugin choose.
fn latex_worker() -> Option<std::path::PathBuf> {
    if std::env::var_os("MANANG_LATEX_WORKER").is_some() {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    sibling_worker(&exe)
}

fn sibling_worker(exe: &std::path::Path) -> Option<std::path::PathBuf> {
    let name = format!("manang-latex-worker{}", std::env::consts::EXE_SUFFIX);
    let candidate = exe.parent()?.join(name);
    candidate.is_file().then_some(candidate)
}

/// Build the kernel with every first-party plugin registered.
///
/// Everything in Manang is a plugin; the shell only dispatches commands.
fn build_kernel(
    dirs: &Dirs<'_>,
    events: &EventBus,
    editor: &Arc<Editor>,
) -> Result<Kernel, BootError> {
    let kernel = Kernel::new();
    kernel.register(
        manang_editor::manifest()?,
        Arc::new(EditorPlugin::new(Arc::clone(editor))),
    )?;
    kernel.register(
        manang_typst::manifest()?,
        Arc::new(manang_typst::plugin(
            manang_typst::CompilerConfig::default(),
        )?),
    )?;
    kernel.register(
        manang_latex::manifest()?,
        Arc::new(manang_latex::plugin(manang_latex::LatexConfig {
            worker: latex_worker(),
            cache_dir: Some(dirs.cache.join("latex")),
            ..manang_latex::LatexConfig::default()
        })),
    )?;
    kernel.register(
        manang_project::manifest()?,
        Arc::new(
            manang_project::ProjectPlugin::new(dirs.config.to_path_buf(), events.clone())
                .with_cache_dir(dirs.cache.to_path_buf()),
        ),
    )?;
    kernel.register(
        manang_figure::manifest()?,
        Arc::new(manang_figure::plugin_with_events(events.clone())),
    )?;
    kernel.register(
        manang_search::manifest()?,
        Arc::new(manang_search::plugin(Some(events.clone()))),
    )?;
    kernel.register(
        manang_git::manifest()?,
        Arc::new(manang_git::plugin(Some(events.clone()))),
    )?;
    kernel.register(
        manang_writing::manifest()?,
        Arc::new(manang_writing::plugin(dirs.config.to_path_buf())),
    )?;
    kernel.register(
        manang_markdown::manifest()?,
        Arc::new(manang_markdown::plugin()),
    )?;
    kernel.register(
        manang_citations::manifest()?,
        Arc::new(manang_citations::plugin()),
    )?;
    kernel.register(
        manang_diagram::manifest()?,
        Arc::new(manang_diagram::plugin()),
    )?;
    // One settings implementation serves three command namespaces.
    let settings = Arc::new(manang_settings::plugin(
        dirs.config.to_path_buf(),
        events.clone(),
    ));
    for manifest in manang_settings::manifests()? {
        kernel.register(
            manifest,
            Arc::clone(&settings) as Arc<dyn manang_kernel::Plugin>,
        )?;
    }
    Ok(kernel)
}

/// Shell state: the kernel plus the WASM plugin host that keeps it extended.
struct Shell {
    kernel: Arc<Kernel>,
    project_root: Arc<ProjectRoot>,
    // Held so the manager and its file watcher live as long as the app.
    _plugins: Arc<PluginManager>,
    _watch: Option<WatchHandle>,
    // Held so plugin settings keep reaching `<plugin>.configure` (ADR 0007).
    _settings: Arc<PluginSettingsSync>,
}

/// Register every plugin's manifest settings, push their values, and keep
/// pushing changes. A plugin that rejects its settings keeps its defaults;
/// that must not stop the app.
fn start_settings_sync(
    kernel: &Arc<Kernel>,
    events: &EventBus,
) -> Result<Arc<PluginSettingsSync>, BootError> {
    let sync = Arc::new(PluginSettingsSync::new(kernel));
    let _ = sync.sync_all();
    sync.start(events)?;
    Ok(sync)
}

/// Load third-party WASM plugins and keep them hot-reloaded.
fn start_plugins(
    kernel: &Arc<Kernel>,
    dirs: &Dirs<'_>,
    events: &EventBus,
    editor: &Arc<Editor>,
    project_root: &Arc<ProjectRoot>,
    settings: &Arc<PluginSettingsSync>,
) -> Result<(Arc<PluginManager>, Option<WatchHandle>), BootError> {
    let mut config = ManagerConfig::new(manang_plugins::plugins_dir_in(dirs.config));
    config.cache_dir = Some(manang_plugins::cache_dir_in(dirs.cache));
    config.services = HostServices {
        buffer: Some(Arc::clone(editor) as Arc<dyn BufferService>),
        project: Arc::clone(project_root),
    };
    let manager = Arc::new(PluginManager::new(Arc::clone(kernel), config)?);

    // Scanning compiles components, which is CPU-bound: never on the UI thread.
    let scanner = Arc::clone(&manager);
    let bus = events.clone();
    let scan_settings = Arc::clone(settings);
    std::thread::Builder::new()
        .name("manang-plugin-scan".to_owned())
        .spawn(move || {
            let topic = match scanner.scan() {
                Ok(report) => {
                    for info in &report.loaded {
                        let _ = scan_settings.sync_plugin(&info.name);
                    }
                    "plugins.scanned"
                }
                Err(_) => "plugins.scan-failed",
            };
            bus.emit(topic, Payload::Empty);
        })
        .map_err(|e| manang_plugins::ManagerError::Watch {
            path: manager.plugins_dir().to_path_buf(),
            reason: e.to_string(),
        })?;

    let bus = events.clone();
    let watch_settings = Arc::clone(settings);
    let watch = manager
        .watch(WatchOptions::default(), move |event| {
            if let PluginEvent::Loaded(info) | PluginEvent::Reloaded(info) = &event {
                let _ = watch_settings.sync_plugin(&info.name);
            }
            let (topic, name) = match &event {
                PluginEvent::Loaded(info) => ("plugins.loaded", info.name.clone()),
                PluginEvent::Reloaded(info) => ("plugins.reloaded", info.name.clone()),
                PluginEvent::Unloaded(name) => ("plugins.unloaded", name.clone()),
                PluginEvent::Failed(_) => ("plugins.failed", String::new()),
                PluginEvent::WatchError(_) => ("plugins.watch-error", String::new()),
            };
            bus.emit(topic, Payload::Json(serde_json::json!({ "name": name })));
        })
        .ok();
    Ok((manager, watch))
}

/// Commands for the palette.
#[tauri::command]
fn kernel_commands(shell: State<'_, Shell>) -> Vec<CommandInfo> {
    shell.kernel.commands()
}

/// One registered plugin, for the Plugins view.
#[derive(Debug, Serialize)]
pub struct PluginSummary {
    pub name: String,
    pub version: String,
    pub description: String,
    pub tier: manang_kernel::Tier,
    pub permissions: manang_kernel::manifest::Permissions,
    /// How many commands it contributes.
    pub commands: usize,
    /// Ids of the settings it declares.
    pub settings: Vec<String>,
}

fn plugin_summaries(kernel: &Kernel) -> Vec<PluginSummary> {
    let mut out: Vec<PluginSummary> = kernel
        .plugin_names()
        .into_iter()
        .filter_map(|name| kernel.manifest(&name))
        .map(|m| PluginSummary {
            name: m.plugin.name,
            version: m.plugin.version.to_string(),
            description: m.plugin.description,
            tier: m.plugin.tier,
            permissions: m.permissions,
            commands: m.contributes.commands.len(),
            settings: m.contributes.settings.into_iter().map(|s| s.id).collect(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Every agent tool plugins declare (ADR 0012), with the declarations that
/// were rejected and why.
#[tauri::command]
fn kernel_agent_tools(shell: State<'_, Shell>) -> manang_kernel::AgentToolList {
    shell.kernel.agent_tools()
}

/// Every registered plugin (first-party and WASM), sorted by name.
#[tauri::command]
fn kernel_plugins(shell: State<'_, Shell>) -> Vec<PluginSummary> {
    plugin_summaries(&shell.kernel)
}

/// Tell the plugins that work on "the open project" which folder it is.
///
/// Best effort: a plugin that is absent or refuses (for example git outside a
/// repository) must not fail opening the project.
fn point_plugins_at_project(kernel: &Kernel, opened: &Payload) {
    let Payload::Json(value) = opened else { return };
    let Some(root) = value.get("root").and_then(Value::as_str) else {
        return;
    };
    for (command, args) in [
        ("git.open", serde_json::json!({ "root": root })),
        ("settings.use-project", serde_json::json!({ "path": root })),
    ] {
        let _ = kernel.invoke(command, Payload::Json(args));
    }
}

/// Run a plugin command off the async runtime. Plugin calls are synchronous
/// and may be CPU-bound, so they never occupy a Tokio worker.
async fn dispatch(
    shell: &Shell,
    command: String,
    args: Option<Value>,
) -> Result<Payload, IpcError> {
    let kernel = Arc::clone(&shell.kernel);
    let input = args.map_or(Payload::Empty, Payload::Json);
    let opening = matches!(command.as_str(), "project.open" | "project.create");
    let started = Instant::now();
    let name = command.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let output = kernel.invoke(&name, input)?;
        if opening {
            point_plugins_at_project(&kernel, &output);
        }
        Ok::<_, KernelError>(output)
    })
    .await
    .map_err(|e| IpcError {
        message: format!("command task failed: {e}"),
    })
    .and_then(|r| r.map_err(IpcError::from));
    log_dispatch(&command, started.elapsed(), result.as_ref().err());
    let output = result?;
    // WASM plugins granted `fs` may only touch the open project's directory.
    if opening
        && let Payload::Json(value) = &output
        && let Some(root) = value.get("root").and_then(Value::as_str)
    {
        shell.project_root.set(Some(root.into()));
    }
    Ok(output)
}

/// `path` resolved (`..` and symlinks included) when it is a file inside
/// `root`; anything else is refused, so the web view cannot have the system
/// open files outside the open project.
fn file_in_project(root: &Path, path: &Path) -> Result<PathBuf, IpcError> {
    let refuse = |why: &str| IpcError {
        message: format!("[outside_project] {}: {why}", path.display()),
    };
    let root = root
        .canonicalize()
        .map_err(|_| refuse("the project folder is not readable"))?;
    let file = path.canonicalize().map_err(|_| refuse("no such file"))?;
    if !file.starts_with(&root) {
        return Err(refuse("not inside the open project"));
    }
    if !file.is_file() {
        return Err(refuse("not a file"));
    }
    Ok(file)
}

/// Open a project file in the program the system associates with it ("Open
/// with system app" on files Manang cannot show). Only files inside the open
/// project; the web view has no general permission to open paths.
#[tauri::command]
async fn open_in_system_app(shell: State<'_, Shell>, path: String) -> Result<(), IpcError> {
    let root = shell.project_root.get().ok_or_else(|| IpcError {
        message: "[no_project] open a project first".to_owned(),
    })?;
    let file = file_in_project(&root, Path::new(&path))?;
    tauri_plugin_opener::open_path(&file, None::<&str>).map_err(|e| {
        log::warn!("open_in_system_app {} failed: {e}", file.display());
        IpcError {
            message: format!("{} could not be opened: {e}", file.display()),
        }
    })
}

/// A command slower than this is logged even when it succeeds.
const SLOW_COMMAND: Duration = Duration::from_millis(200);

/// Failures and slow commands always, so the terminal running `pnpm app:dev`
/// shows what went wrong and what was slow; every command only at trace level
/// (one per keystroke would drown the rest).
fn log_dispatch(command: &str, took: Duration, error: Option<&IpcError>) {
    let ms = took.as_secs_f64() * 1000.0;
    match error {
        Some(e) => log::warn!("{command} failed after {ms:.1} ms: {}", e.message),
        None if took >= SLOW_COMMAND => log::info!("{command} slow: {ms:.1} ms"),
        None => log::trace!("{command} {ms:.1} ms"),
    }
}

/// Open the web inspector (F12 / Ctrl+Shift+I). Debug builds only; a release
/// build has no inspector and ignores the call.
#[tauri::command]
fn open_devtools(window: tauri::WebviewWindow) {
    #[cfg(debug_assertions)]
    window.open_devtools();
    #[cfg(not(debug_assertions))]
    let _ = window;
}

/// Run a command whose result is structured data.
#[tauri::command]
async fn kernel_invoke(
    shell: State<'_, Shell>,
    command: String,
    args: Option<Value>,
) -> Result<Value, IpcError> {
    match dispatch(&shell, command.clone(), args).await? {
        Payload::Json(value) => Ok(value),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(IpcError {
            message: format!("`{command}` returns binary; use kernel_invoke_binary"),
        }),
    }
}

/// Run a command whose result is a binary artefact (PDF, SVG). The bytes cross
/// IPC raw, never as base64 or JSON.
#[tauri::command]
async fn kernel_invoke_binary(
    shell: State<'_, Shell>,
    command: String,
    args: Option<Value>,
) -> Result<tauri::ipc::Response, IpcError> {
    match dispatch(&shell, command.clone(), args).await? {
        Payload::Binary(bytes) => Ok(tauri::ipc::Response::new(bytes.to_vec())),
        _ => Err(IpcError {
            message: format!("`{command}` does not return binary"),
        }),
    }
}

/// Validate a destination chosen in the native Save dialog: an absolute path
/// whose folder exists, that is not itself a directory.
fn check_destination(dest: &std::path::Path) -> Result<(), IpcError> {
    let fail = |message: String| Err(IpcError { message });
    if !dest.is_absolute() {
        return fail(format!("`{}` is not an absolute path", dest.display()));
    }
    if dest.is_dir() {
        return fail(format!("`{}` is a folder", dest.display()));
    }
    match dest.parent() {
        Some(parent) if parent.is_dir() => Ok(()),
        _ => fail(format!(
            "the folder for `{}` does not exist",
            dest.display()
        )),
    }
}

/// Run a binary kernel command (for example `typst.export-pdf`) and write its
/// bytes to `dest_path`, chosen by the user in the native Save dialog. The
/// bytes never cross IPC: the kernel produces them and this writes them,
/// atomically, on a blocking thread. Resolves to the number of bytes written.
#[tauri::command]
async fn save_binary_artifact(
    shell: State<'_, Shell>,
    command: String,
    args: Option<Value>,
    dest_path: String,
) -> Result<u64, IpcError> {
    let dest = std::path::PathBuf::from(dest_path);
    check_destination(&dest)?;
    let bytes = match dispatch(&shell, command.clone(), args).await? {
        Payload::Binary(bytes) => bytes,
        _ => {
            return Err(IpcError {
                message: format!("`{command}` does not return binary"),
            });
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        manang_project::fsops::atomic_write(&dest, &bytes)
            .map(|()| bytes.len() as u64)
            .map_err(|e| IpcError {
                message: e.to_string(),
            })
    })
    .await
    .map_err(|e| IpcError {
        message: format!("save task failed: {e}"),
    })?
}

/// Forward kernel events (file changes, ...) to the shell as `kernel://event`.
fn forward_events(app: tauri::AppHandle, events: &EventBus) -> std::io::Result<()> {
    let subscription = events.subscribe("*");
    std::thread::Builder::new()
        .name("manang-events".to_owned())
        .spawn(move || {
            while let Some(event) = subscription.recv() {
                let payload = match event.payload {
                    Payload::Json(value) => value,
                    _ => Value::Null,
                };
                // A closed window is not an error worth stopping the pump for.
                let _ = app.emit(
                    "kernel://event",
                    serde_json::json!({ "topic": event.topic, "payload": payload }),
                );
            }
        })
        .map(|_| ())
}

/// WebKitGTK's DMABUF renderer misdraws under Wayland with NVIDIA's driver:
/// parts of the page do not paint, scrolling stutters, clicks miss. Returns the
/// variable and value that turn it off when that is the situation and the user
/// has not chosen either way (`WEBKIT_DISABLE_DMABUF_RENDERER=0` keeps it).
pub fn webkit_renderer_override(
    wayland: bool,
    nvidia: bool,
    already_set: bool,
) -> Option<(&'static str, &'static str)> {
    (cfg!(target_os = "linux") && wayland && nvidia && !already_set)
        .then_some(("WEBKIT_DISABLE_DMABUF_RENDERER", "1"))
}

static RENDERER_OVERRIDDEN: AtomicBool = AtomicBool::new(false);

/// `main` applied [`webkit_renderer_override`]; the log says so at startup.
pub fn note_renderer_override() {
    RENDERER_OVERRIDDEN.store(true, Ordering::Relaxed);
}

/// The inputs of [`webkit_renderer_override`] for this machine.
pub fn webkit_renderer_situation() -> (bool, bool, bool) {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland");
    let nvidia = std::path::Path::new("/proc/driver/nvidia/version").exists();
    let already_set = std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some();
    (wayland, nvidia, already_set)
}

/// How much the shell itself logs: everything while developing.
const OWN_LEVEL: log::LevelFilter = if cfg!(debug_assertions) {
    log::LevelFilter::Debug
} else {
    log::LevelFilter::Info
};

/// Panics are bugs: put them in the log (with where they happened) before the
/// default hook prints them, so they land in the log file too.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        default(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), tauri::Error> {
    log_panics();
    let builder = tauri::Builder::default();
    // First, so a second launch hands over to the running window and exits
    // before it builds a second kernel.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        use tauri::Manager;
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }));
    builder
        // Logs go to the terminal and the platform log folder. The web view
        // forwards its errors and warnings here too (src/lib/kernel/log.ts).
        // Debug builds log the shell's own debug messages too.
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("manang_lib", OWN_LEVEL)
                .max_file_size(2_000_000)
                .build(),
        )
        // Size, position and maximised state survive a restart.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            use tauri::Manager;
            if RENDERER_OVERRIDDEN.load(Ordering::Relaxed) {
                log::info!(
                    "WebKit DMABUF renderer off (Wayland with NVIDIA); \
                     set WEBKIT_DISABLE_DMABUF_RENDERER=0 to keep it"
                );
            }
            let config_dir = app.path().app_config_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            let dirs = Dirs {
                config: &config_dir,
                cache: &cache_dir,
            };
            let events = EventBus::new();
            let editor = Arc::new(Editor::new());
            let kernel = Arc::new(build_kernel(&dirs, &events, &editor)?);
            let project_root = Arc::new(ProjectRoot::new(None));
            let settings = start_settings_sync(&kernel, &events)?;
            let (plugins, watch) =
                start_plugins(&kernel, &dirs, &events, &editor, &project_root, &settings)?;
            forward_events(app.handle().clone(), &events)?;
            app.manage(Shell {
                kernel,
                project_root,
                _plugins: plugins,
                _watch: watch,
                _settings: settings,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            kernel_commands,
            kernel_plugins,
            kernel_agent_tools,
            kernel_invoke,
            kernel_invoke_binary,
            save_binary_artifact,
            open_devtools,
            open_in_system_app
        ])
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    use super::{AppInfo, Arc, Dirs, Editor, EventBus, app_info, build_kernel, check_destination};
    use manang_kernel::Payload;
    use serde_json::json;

    #[test]
    fn save_destination_must_be_an_absolute_file_in_an_existing_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(check_destination(&dir.path().join("out.pdf")).is_ok());
        assert!(check_destination(std::path::Path::new("out.pdf")).is_err());
        assert!(check_destination(dir.path()).is_err());
        assert!(check_destination(&dir.path().join("nope/out.pdf")).is_err());
    }

    #[test]
    fn only_files_inside_the_project_can_be_opened_by_the_system() {
        use super::file_in_project;
        let project = tempfile::tempdir().expect("project");
        let outside = tempfile::tempdir().expect("outside");
        let inside = project.path().join("data.zip");
        std::fs::write(&inside, b"x").expect("write");
        let secret = outside.path().join("secret.sh");
        std::fs::write(&secret, b"x").expect("write");

        assert!(file_in_project(project.path(), &inside).is_ok());
        let escape = project.path().join("..").join(
            outside
                .path()
                .file_name()
                .expect("name")
                .to_str()
                .expect("utf8"),
        );
        assert!(file_in_project(project.path(), &escape.join("secret.sh")).is_err());
        assert!(file_in_project(project.path(), &secret).is_err());
        assert!(
            file_in_project(project.path(), project.path()).is_err(),
            "a folder"
        );
        assert!(file_in_project(project.path(), &project.path().join("nope")).is_err());
        #[cfg(unix)]
        {
            let link = project.path().join("link.sh");
            std::os::unix::fs::symlink(&secret, &link).expect("symlink");
            assert!(
                file_in_project(project.path(), &link).is_err(),
                "symlink out"
            );
        }
    }

    #[test]
    fn dmabuf_renderer_is_turned_off_only_for_wayland_on_nvidia_when_unset() {
        use super::webkit_renderer_override as decide;
        let off = Some(("WEBKIT_DISABLE_DMABUF_RENDERER", "1"));
        let expected = if cfg!(target_os = "linux") { off } else { None };
        assert_eq!(decide(true, true, false), expected);
        assert_eq!(decide(false, true, false), None, "X11 is fine");
        assert_eq!(decide(true, false, false), None, "AMD and Intel are fine");
        assert_eq!(decide(true, true, true), None, "the user's choice wins");
    }

    #[test]
    fn app_info_reports_build_identity() {
        let info: AppInfo = app_info();

        assert_eq!(info.name, "Manang");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.target, "typst");
    }

    #[test]
    fn worker_is_found_next_to_the_executable_only_when_present() {
        let dir = std::env::temp_dir().join(format!("manang-worker-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let exe = dir.join("manang");
        assert_eq!(super::sibling_worker(&exe), None);

        let name = format!("manang-latex-worker{}", std::env::consts::EXE_SUFFIX);
        let worker = dir.join(name);
        std::fs::write(&worker, b"").expect("write worker stub");
        assert_eq!(super::sibling_worker(&exe), Some(worker));
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn app_info_serialises_as_structured_data() {
        // The shell must never parse strings out of the IPC boundary.
        let json = serde_json::to_value(app_info()).expect("app_info serialises");

        assert_eq!(json["name"], "Manang");
        assert!(json["version"].is_string());
        assert!(json["target"].is_string());
    }

    #[test]
    fn kernel_boots_with_the_editor_plugin() {
        let tmp = std::env::temp_dir();
        let dirs = Dirs {
            config: &tmp,
            cache: &tmp,
        };
        let kernel = build_kernel(&dirs, &EventBus::new(), &Arc::new(Editor::new()))
            .expect("first-party plugins register");

        let ids: Vec<_> = kernel.commands().into_iter().map(|c| c.id).collect();
        for expected in [
            "editor.new",
            "typst.compile",
            "latex.compile",
            "project.open",
        ] {
            assert!(ids.iter().any(|id| id == expected), "missing {expected}");
        }
        let out = kernel
            .invoke("editor.new", Payload::Empty)
            .expect("editor.new runs");
        assert_eq!(out, Payload::Json(json!({ "id": 1 })));
    }

    #[test]
    fn diagram_plugin_registers_and_renders_svg() {
        let tmp = std::env::temp_dir();
        let dirs = Dirs {
            config: &tmp,
            cache: &tmp,
        };
        let kernel = build_kernel(&dirs, &EventBus::new(), &Arc::new(Editor::new()))
            .expect("first-party plugins register");
        let ids: Vec<_> = kernel.commands().into_iter().map(|c| c.id).collect();
        for expected in [
            "diagram.languages",
            "diagram.check",
            "diagram.render",
            "diagram.key",
        ] {
            assert!(ids.iter().any(|id| id == expected), "missing {expected}");
        }
        let out = kernel
            .invoke(
                "diagram.render",
                Payload::Json(json!({
                    "language": "mermaid",
                    "source": "flowchart LR\n  A --> B\n",
                    "theme": "dark",
                })),
            )
            .expect("diagram.render runs");
        let Payload::Binary(svg) = out else {
            panic!("diagram.render returns binary SVG");
        };
        assert!(svg.starts_with(b"<svg"));
    }

    #[test]
    fn first_party_agent_tools_are_all_valid() {
        let tmp = std::env::temp_dir();
        let dirs = Dirs {
            config: &tmp,
            cache: &tmp,
        };
        let kernel = build_kernel(&dirs, &EventBus::new(), &Arc::new(Editor::new()))
            .expect("first-party plugins register");
        let list = kernel.agent_tools();
        assert!(list.problems.is_empty(), "{:?}", list.problems);
        assert!(
            list.tools
                .iter()
                .any(|t| t.tool.name == "figure_add_shapes")
        );
    }

    #[test]
    fn first_party_settings_register_and_reach_their_plugins() {
        let config = tempfile::tempdir().expect("tempdir");
        let dirs = Dirs {
            config: config.path(),
            cache: config.path(),
        };
        let kernel = Arc::new(
            build_kernel(&dirs, &EventBus::new(), &Arc::new(Editor::new()))
                .expect("first-party plugins register"),
        );
        let sync = manang_settings::PluginSettingsSync::new(&kernel);
        for (name, result) in sync.sync_all() {
            let outcome = result.unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                outcome.pushed,
                outcome.declared > 0,
                "{name} declares settings but has no configure command"
            );
        }
        let Ok(Payload::Json(schema)) = kernel.invoke("settings.schema", Payload::Empty) else {
            panic!("schema");
        };
        let engine = schema["settings"]
            .as_array()
            .and_then(|s| s.iter().find(|d| d["id"] == "latex.engine"))
            .expect("latex.engine registered");
        assert_eq!(engine["owner"], "latex");
        assert_eq!(engine["category"], "latex");
        assert_eq!(engine["scope"], "project");
        assert_eq!(engine["title"], "Engine");

        let summaries = super::plugin_summaries(&kernel);
        let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        let latex = summaries
            .iter()
            .find(|s| s.name == "latex")
            .expect("latex listed");
        assert!(latex.settings.contains(&"latex.engine".to_owned()));
        let json = serde_json::to_value(latex).expect("serialises");
        assert_eq!(json["tier"], "native");
        assert!(json["commands"].as_u64().is_some_and(|n| n > 0));
        assert!(json["permissions"]["filesystem"].is_array());
    }
}
