//! The `agent.*` commands.

use crate::commands::{self, SlashCommand};
use crate::context::{
    HeaderInfo, MENTION_CAP, Mention, ResolvedPart, SELECTION_CAP, TurnInput, View,
    diagnostics_text, load_instructions, render_header, render_turn, resolve_part,
};
use crate::error::AgentError;
use crate::prompt::SYSTEM_PROMPT;
use crate::provider::{ProviderFactory, ProviderSpec, RealFactory, friendly};
use crate::risk::{Decision, RiskTable};
use crate::session::{TurnPlan, commit, run};
use crate::settings::{Settings, check_offline, check_url};
use crate::state::{
    Limits, ProposalState, Session, SessionInner, Shared, SharedInit, valid_session_id,
};
use crate::storage::{
    GlobalStore, SessionMeta, SessionStore, list_sessions, now_ms, project_agent_dir,
    prune_sessions, read_project_policy,
};
use crate::text::{clip_chars, estimate_tokens};
use crate::tools::{self, ToolContext};
use manang_agent_core::{
    AuditEntry, AuditEvent, ChatRequest, EffectivePolicy, Message, PolicyLayer, PriceTable,
    ProviderEvent, ProviderKind, ProviderPreset, RedactionConfig, ToolNameMap, ToolSpec, Usage,
    preset_by_id, presets, redact,
};
use manang_kernel::{Context, Hook, Manifest, ManifestError, Payload, Plugin, PluginError};
use manang_keystore::{KeyCommands, KeyId, SecretString};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const MANIFEST: &str = include_str!("../manifest.toml");
const MAX_PROMPT_CHARS: usize = 20_000;
const MAX_RUNNING: usize = 4;
/// Entries of the file tree in the session header.
const HEADER_FILES: usize = 60;
/// Open buffers the UI may send, and the biggest one.
const MAX_BUFFERS: usize = 40;
const MAX_BUFFER_BYTES: usize = 2_000_000;

/// The plugin's manifest.
pub fn manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// How the shell builds the plugin.
pub struct AgentOptions {
    pub config_dir: PathBuf,
    pub events: manang_kernel::EventBus,
    pub keys: Arc<KeyCommands>,
    pub factory: Arc<dyn ProviderFactory>,
    pub risk: RiskTable,
    pub limits: Limits,
}

impl AgentOptions {
    /// Production defaults: the best key store for this machine and the real providers.
    pub fn new(config_dir: PathBuf, events: manang_kernel::EventBus) -> Self {
        Self {
            config_dir,
            events,
            keys: Arc::new(KeyCommands::new(Arc::new(manang_keystore::detect()))),
            factory: Arc::new(RealFactory),
            risk: RiskTable::default(),
            limits: Limits::default(),
        }
    }
}

/// Builds the plugin with production defaults.
pub fn plugin(
    config_dir: PathBuf,
    events: manang_kernel::EventBus,
) -> Result<AgentPlugin, AgentError> {
    AgentPlugin::new(AgentOptions::new(config_dir, events))
}

/// The plugin. Call [`AgentPlugin::attach`] once the kernel is in an `Arc`.
pub struct AgentPlugin {
    shared: Arc<Shared>,
    runtime: Mutex<Option<tokio::runtime::Runtime>>,
}

impl Drop for AgentPlugin {
    fn drop(&mut self) {
        if let Some(rt) = self.runtime.lock().take() {
            rt.shutdown_background();
        }
    }
}

fn args_of(input: Payload) -> Result<Value, AgentError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(json!({})),
        Payload::Binary(_) => Err(AgentError::invalid("expected a JSON payload")),
    }
}

fn parse<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, AgentError> {
    serde_json::from_value(value).map_err(|e| AgentError::invalid(e.to_string()))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    model: Option<String>,
    max_iterations: Option<u32>,
    max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartArgs {
    #[serde(default)]
    session_id: Option<String>,
    /// What the author typed (after the command name, when there is one).
    #[serde(default)]
    prompt: Option<String>,
    /// A slash command: its template becomes the author's message.
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    context: TurnInput,
    #[serde(default)]
    options: Options,
    #[serde(default)]
    project_root: Option<String>,
    /// Run the conversation again as it stands (after a connection error): nothing is appended.
    #[serde(default)]
    retry: bool,
}

/// An @-mention resolved to text.
struct MentionText {
    kind: &'static str,
    label: String,
    path: Option<String>,
    body: String,
}

struct Prepared {
    settings: Settings,
    preset: ProviderPreset,
    spec: ProviderSpec,
    model: String,
    parts: Vec<ResolvedPart>,
    prompt_redactions: u32,
    system: String,
    tools: Vec<ToolSpec>,
    names: ToolNameMap,
    user_message: Message,
    /// The author's message as sent (after command expansion and redaction), for the title.
    request_text: String,
    header: String,
    existing: Option<Arc<Session>>,
    project_root: Option<PathBuf>,
    options: Options,
    redaction: RedactionConfig,
    context_tokens: u64,
    window: u64,
    effort: Option<String>,
    tool_ctx: ToolContext,
}

impl AgentPlugin {
    pub fn new(options: AgentOptions) -> Result<Self, AgentError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("manang-agent")
            .enable_all()
            .build()
            .map_err(|e| AgentError::unavailable(format!("the agent runtime failed: {e}")))?;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .thread_name(|i| format!("manang-agent-tool-{i}"))
            .build()
            .map_err(|e| AgentError::unavailable(format!("the tool pool failed: {e}")))?;
        let shared = Arc::new(Shared::new(SharedInit {
            config_dir: options.config_dir.clone(),
            events: options.events,
            global: GlobalStore::new(&options.config_dir),
            keys: options.keys,
            factory: options.factory,
            risk: options.risk,
            limits: options.limits,
            rt: runtime.handle().clone(),
            pool,
        }));
        Ok(Self {
            shared,
            runtime: Mutex::new(Some(runtime)),
        })
    }

    /// Connects the plugin to the kernel it is registered in (tools run through it).
    pub fn attach(&self, kernel: &Arc<manang_kernel::Kernel>) {
        *self.shared.kernel.lock() = Arc::downgrade(kernel);
    }

    /// Registers the plugin in `kernel` and attaches it.
    pub fn register(
        self,
        kernel: &Arc<manang_kernel::Kernel>,
    ) -> Result<Arc<Self>, manang_kernel::KernelError> {
        let me = Arc::new(self);
        kernel.register(manifest()?, Arc::clone(&me) as Arc<dyn Plugin>)?;
        me.attach(kernel);
        Ok(me)
    }

    fn block_on<F, T>(&self, fut: F) -> Result<T, AgentError>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        self.shared.rt.spawn(async move {
            let _ = tx.send(fut.await);
        });
        rx.recv()
            .map_err(|_| AgentError::unavailable("the agent runtime stopped"))
    }

    // ---- policy and provider -------------------------------------------------------------

    fn root_of(&self, given: Option<&str>) -> Option<PathBuf> {
        let root = given
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| self.shared.last_project.lock().clone());
        if let Some(r) = &root {
            *self.shared.last_project.lock() = Some(r.clone());
        }
        root
    }

    fn sessions_dir(&self, root: Option<&Path>) -> (PathBuf, bool) {
        match root {
            Some(r) => (project_agent_dir(r).join("sessions"), true),
            None => (self.shared.global.sessions_dir(), false),
        }
    }

    /// The settings and the merged policy: enterprise (permissive placeholder), user, project.
    fn policy(&self, root: Option<&Path>) -> Result<(Settings, EffectivePolicy), AgentError> {
        let settings = self.shared.settings.read().clone();
        let project = match root {
            Some(r) => read_project_policy(r)?,
            None => None,
        };
        let mut layers = vec![PolicyLayer::default(), settings.policy_layer()];
        layers.extend(project);
        let policy = EffectivePolicy::merge(&layers);
        if !policy.enabled {
            let why = if settings.enabled {
                "this project's AI policy switches the agent off"
            } else {
                "the AI agent is turned off in Settings"
            };
            return Err(AgentError::disabled(why));
        }
        Ok((settings, policy))
    }

    fn key_for(&self, preset: &ProviderPreset) -> Result<Option<SecretString>, AgentError> {
        let id = KeyId::for_provider(&preset.id)
            .map_err(|e| AgentError::new("key_unavailable", e.to_string()))?;
        self.shared
            .keys
            .store()
            .get(&id)
            .map_err(|e| AgentError::new("key_unavailable", e.to_string()))
    }

    fn spec(
        &self,
        settings: &Settings,
        policy: &EffectivePolicy,
        require_key: bool,
        effort: Option<&str>,
    ) -> Result<(ProviderSpec, ProviderPreset), AgentError> {
        let preset = settings.preset()?;
        policy.check_provider(&preset.id)?;
        let base_url = if settings.base_url.is_empty() {
            preset.base_url.clone().ok_or_else(|| {
                AgentError::invalid("this provider needs a base URL: set it in Settings")
            })?
        } else {
            settings.base_url.clone()
        };
        check_url(&base_url)?;
        policy.check_base_url(&base_url)?;
        if settings.offline {
            check_offline(&base_url)?;
        }
        let mut api_key = self.key_for(&preset)?;
        if api_key.is_none() {
            if preset.key_required && require_key {
                return Err(AgentError::new(
                    "key_missing",
                    format!(
                        "no API key is saved for {}: add one in Settings",
                        preset.display_name
                    ),
                ));
            }
            if preset.key_required {
                // Previews mask the key; any placeholder shows the same request.
                api_key = Some(SecretString::from("preview".to_owned()));
            }
        }
        Ok((
            ProviderSpec {
                preset: preset.clone(),
                base_url,
                api_key,
                offline: settings.offline,
                engine: settings.engine,
                effort: effort.map(str::to_owned),
            },
            preset,
        ))
    }

    fn model_of(
        settings: &Settings,
        preset: &ProviderPreset,
        options: &Options,
    ) -> Result<String, AgentError> {
        options
            .model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .or_else(|| Some(settings.model.clone()).filter(|m| !m.is_empty()))
            .or_else(|| preset.models.first().cloned())
            .ok_or_else(|| AgentError::invalid("choose a model in Settings"))
    }

    // ---- context ---------------------------------------------------------------------------

    /// The text of an @-mentioned file when the UI sent none.
    fn mention_text(
        &self,
        ctx: &Context<'_>,
        mention: &Mention,
    ) -> Result<Option<MentionText>, AgentError> {
        match mention {
            Mention::File { path, text } => {
                let path = tools::clean_path(path).map_err(AgentError::invalid)?;
                let body = match text {
                    Some(t) => t.clone(),
                    None => match ctx
                        .invoke("project.read-file", Payload::Json(json!({ "path": path })))
                    {
                        Ok(Payload::Json(Value::String(t))) => t,
                        _ => {
                            return Err(AgentError::not_found(format!("{path} could not be read")));
                        }
                    },
                };
                Ok(Some(MentionText {
                    kind: "file",
                    label: path.clone(),
                    path: Some(path),
                    body,
                }))
            }
            Mention::Problems { diagnostics } => {
                if diagnostics.is_empty() {
                    return Ok(None);
                }
                Ok(Some(MentionText {
                    kind: "problems",
                    label: "Problems".to_owned(),
                    path: None,
                    body: diagnostics_text(diagnostics),
                }))
            }
            Mention::Figure { path } => {
                let args = path.as_ref().map_or(json!({}), |p| json!({ "path": p }));
                let out = ctx
                    .invoke("figure.describe", Payload::Json(args))
                    .map_err(|e| {
                        AgentError::unavailable(format!("the figure could not be described: {e}"))
                    })?;
                let Payload::Json(v) = out else {
                    return Err(AgentError::unavailable("the figure has no description"));
                };
                let summary = v
                    .get("summary")
                    .and_then(Value::as_str)
                    .map_or_else(|| v.to_string(), str::to_owned);
                Ok(Some(MentionText {
                    kind: "figure",
                    label: format!("Figure {}", path.as_deref().unwrap_or("")),
                    path: path.clone(),
                    body: summary,
                }))
            }
        }
    }

    /// The session header: project name, formats, file tree, instructions, problem count.
    fn header(
        &self,
        ctx: &Context<'_>,
        tc: &ToolContext,
        settings: &Settings,
        problems: usize,
        redaction: &RedactionConfig,
    ) -> String {
        let files = tools::project_files(ctx, tc, "").unwrap_or_default();
        let shown = &files[..files.len().min(HEADER_FILES)];
        let mut formats: Vec<String> = Vec::new();
        for (ext, name) in [
            ("typ", "Typst"),
            ("tex", "LaTeX"),
            ("md", "Markdown"),
            ("bib", "BibTeX"),
            ("figure", "figures"),
        ] {
            if files.iter().any(|f| f.path.ends_with(&format!(".{ext}"))) {
                formats.push(name.to_owned());
            }
        }
        let mut tree = tools::render_tree(shown);
        if files.len() > shown.len() {
            tree.push_str(&format!(
                "[{} more files: use list_files with dir]\n",
                files.len() - shown.len()
            ));
        }
        let name = tc
            .project_root
            .as_ref()
            .and_then(|r| r.file_name())
            .map_or_else(
                || "project".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
        let instructions = load_instructions(
            tc.project_root.as_deref(),
            &self.shared.config_dir,
            settings.use_agents_md,
        )
        .into_iter()
        .map(|(source, text)| (source, redact(&text, redaction).text))
        .collect();
        render_header(&HeaderInfo {
            name,
            formats,
            tree,
            shown: shown.len(),
            total: files.len(),
            problems,
            instructions,
        })
    }

    fn prepare(
        &self,
        ctx: &Context<'_>,
        args: StartArgs,
        sending: bool,
    ) -> Result<Prepared, AgentError> {
        let root = self.root_of(args.project_root.as_deref());
        let (settings, policy) = self.policy(root.as_deref())?;
        let existing = match &args.session_id {
            Some(id) => Some(self.load_session(id, root.as_deref())?),
            None => None,
        };
        let command = match args.command.as_deref().filter(|c| !c.is_empty()) {
            Some(name) => Some(
                self.slash_commands(root.as_deref())
                    .into_iter()
                    .find(|c| c.name == name)
                    .ok_or_else(|| {
                        AgentError::new("unknown_command", format!("there is no command `/{name}`"))
                    })?,
            ),
            None => None,
        };
        // Reasoning effort is fixed when the session starts.
        let effort = match &existing {
            Some(s) if s.inner.lock().effort.is_some() => s.inner.lock().effort.clone(),
            _ => Some(
                settings
                    .reasoning_effort
                    .resolve(command.is_some())
                    .to_owned(),
            ),
        };
        let (spec, preset) = self.spec(&settings, &policy, sending, effort.as_deref())?;
        if sending && !self.shared.global.has_consent(&preset.id) {
            return Err(AgentError::new(
                "consent_required",
                format!(
                    "agree to send data to {} first (Settings, Provider and key)",
                    preset.display_name
                ),
            ));
        }
        let model = Self::model_of(&settings, &preset, &args.options)?;
        let typed = args.prompt.clone().unwrap_or_default();
        if typed.chars().count() > MAX_PROMPT_CHARS {
            return Err(AgentError::invalid("the message is too long"));
        }
        let request_text = match &command {
            Some(c) => commands::expand(c, &typed),
            None => typed.trim().to_owned(),
        };
        if request_text.is_empty() && !args.retry {
            return Err(AgentError::invalid("write a message first"));
        }
        let redaction = RedactionConfig {
            secrets: true,
            emails: settings.redact_emails,
            paths: settings.redact_paths,
        };
        let turn = args.context;

        // What the tools may look at: the editor's buffers, then the disk.
        let mut buffers: BTreeMap<String, String> = BTreeMap::new();
        for (path, body) in &turn.buffers {
            if buffers.len() >= MAX_BUFFERS || body.len() > MAX_BUFFER_BYTES {
                continue;
            }
            if let Ok(clean) = tools::clean_path(path) {
                buffers.insert(clean, body.clone());
            }
        }
        let tool_ctx = ToolContext {
            session_id: existing.as_ref().map_or_else(String::new, |s| s.id.clone()),
            project_root: root.clone(),
            active: turn.active.as_ref().map(|a| a.path.clone()),
            buffers,
            diagnostics: turn.diagnostics.clone(),
            // A key the author's message or a problem they pointed at names may be changed.
            user_text: std::iter::once(request_text.as_str())
                .chain(turn.diagnostics.iter().map(|d| d.message.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
            pending: Vec::new(),
        };

        // Turn context: the selection and the @-mentions.
        let mut parts: Vec<ResolvedPart> = Vec::new();
        if let Some(sel) = &turn.selection {
            let mut part = resolve_part(
                "selection",
                "Selection",
                Some(&sel.path),
                &sel.text,
                SELECTION_CAP,
                &redaction,
            );
            part.lines = sel
                .line_from
                .zip(sel.line_to)
                .or(sel.line_from.map(|l| (l, l)));
            parts.push(part);
        }
        for mention in &turn.mentions {
            if let Some(m) = self.mention_text(ctx, mention)? {
                parts.push(resolve_part(
                    m.kind,
                    &m.label,
                    m.path.as_deref(),
                    &m.body,
                    MENTION_CAP,
                    &redaction,
                ));
            }
        }

        let request_redaction = redact(&request_text, &redaction);
        let context_tokens =
            parts.iter().map(|p| p.tokens).sum::<u64>() + estimate_tokens(&request_redaction.text);
        if let Some(limit) = policy.max_context_tokens
            && context_tokens > limit
        {
            return Err(AgentError::new(
                "context_too_large",
                format!(
                    "the attached context is about {context_tokens} tokens; this project's limit is {limit}. Select less text or remove a mention."
                ),
            ));
        }

        // The session header goes in the first message only.
        let first = existing
            .as_ref()
            .is_none_or(|s| s.inner.lock().history.is_empty());
        let header = if first {
            let mut header_ctx = tool_ctx.clone();
            header_ctx.session_id.clear();
            self.header(
                ctx,
                &header_ctx,
                &settings,
                turn.diagnostics.len(),
                &redaction,
            )
        } else {
            String::new()
        };
        let turn_text = render_turn(&request_redaction.text, turn.active.as_ref(), &parts);
        let user_text = if header.is_empty() {
            turn_text
        } else {
            format!("{header}\n\n{turn_text}")
        };
        let user_message = Message::user_text(&user_text)?;

        let kernel = self.shared.kernel()?;
        let figure_view = turn.view == View::Figure;
        let mut tools = Vec::new();
        for registered in kernel.agent_tools().tools {
            let t = registered.tool;
            let Some(info) = self.shared.risk.get(&t.name) else {
                continue;
            };
            if t.name.starts_with("figure_") && !figure_view {
                continue;
            }
            // What goes on the wire: a name, a description and an input schema. No title, no
            // output schema (the model never reads them, and they cost tokens on every request).
            tools.push(ToolSpec {
                name: t.name,
                title: None,
                description: t.description,
                input_schema: t.input_schema,
                output_schema: None,
                risk: info.risk,
            });
        }
        let names = ToolNameMap::new(tools.iter().map(|t| t.name.as_str()))?;
        let window = policy
            .max_context_tokens
            .map_or(settings.context_window, |m| m.min(settings.context_window));
        Ok(Prepared {
            system: SYSTEM_PROMPT.to_owned(),
            prompt_redactions: u32::try_from(request_redaction.spans.len()).unwrap_or(u32::MAX),
            settings,
            preset,
            spec,
            model,
            parts,
            tools,
            names,
            user_message,
            request_text: request_redaction.text,
            header,
            existing,
            project_root: root,
            options: args.options,
            redaction,
            context_tokens,
            window,
            effort,
            tool_ctx,
        })
    }

    fn plan(&self, p: &Prepared) -> Result<TurnPlan, AgentError> {
        let provider = self.shared.factory.build(&p.spec)?;
        let redaction_count: usize = p.parts.iter().map(|c| c.redactions.len()).sum::<usize>()
            + p.prompt_redactions as usize;
        Ok(TurnPlan {
            provider,
            provider_id: p.preset.id.clone(),
            model: p.model.clone(),
            system: p.system.clone(),
            tools: p.tools.clone(),
            names: p.names.clone(),
            max_tokens: p.options.max_tokens.unwrap_or(8192).clamp(256, 32_768),
            max_iterations: p.options.max_iterations.unwrap_or(12).clamp(1, 24),
            window: p.window,
            include_thinking: p.settings.include_thinking,
            cache: p.preset.kind == ProviderKind::Anthropic,
            redaction: p.redaction,
            redaction_count: u32::try_from(redaction_count).unwrap_or(u32::MAX),
            prices: PriceTable::builtin(),
        })
    }

    fn slash_commands(&self, root: Option<&Path>) -> Vec<SlashCommand> {
        commands::load(root, &self.shared.config_dir)
    }

    // ---- sessions --------------------------------------------------------------------------

    fn load_session(&self, id: &str, root: Option<&Path>) -> Result<Arc<Session>, AgentError> {
        if !valid_session_id(id) {
            return Err(AgentError::invalid("that is not a session id"));
        }
        if let Some(s) = self.shared.sessions.lock().get(id) {
            return Ok(Arc::clone(s));
        }
        let (dir, _) = self.sessions_dir(root);
        let store = SessionStore::open(&dir, id)
            .or_else(|| SessionStore::open(&self.shared.global.sessions_dir(), id))
            .ok_or_else(|| AgentError::not_found(format!("no session `{id}`")))?;
        let entries = store.read_transcript()?;
        let meta = store.read_meta();
        let history: Vec<Message> = entries.iter().map(|e| e.message.clone()).collect();
        let header = header_of(&history);
        let session = Arc::new(Session {
            id: id.to_owned(),
            inner: Mutex::new(SessionInner {
                seq: entries.last().map_or(0, |e| e.seq),
                view: history.clone(),
                history,
                header,
                cleared: false,
                last_fill: 0,
                effort: None,
                running: false,
                cancel: None,
                remembered: BTreeSet::new(),
                tainted: false,
                usage: Usage::default(),
                cost_micro: 0,
                provider: meta
                    .as_ref()
                    .map_or_else(String::new, |m| m.provider.clone()),
                model: meta.as_ref().map_or_else(String::new, |m| m.model.clone()),
                title: meta.as_ref().map_or_else(String::new, |m| m.title.clone()),
                created_ms: meta.as_ref().map_or_else(now_ms, |m| m.created_ms),
                turns: meta.map_or(0, |m| m.turns),
            }),
            store,
        });
        self.shared
            .sessions
            .lock()
            .insert(id.to_owned(), Arc::clone(&session));
        Ok(session)
    }

    fn start(&self, ctx: &Context<'_>, value: Value) -> Result<Value, AgentError> {
        let args: StartArgs = parse(value)?;
        let retry = args.retry;
        let prepared = self.prepare(ctx, args, true)?;
        let running = self
            .shared
            .sessions
            .lock()
            .values()
            .filter(|s| s.inner.lock().running)
            .count();
        if running >= MAX_RUNNING {
            return Err(AgentError::new(
                "busy",
                "too many sessions are running; stop one first",
            ));
        }
        let plan = self.plan(&prepared)?;
        let (dir, in_project) = self.sessions_dir(prepared.project_root.as_deref());
        let session = match &prepared.existing {
            Some(s) => Arc::clone(s),
            None => {
                prune_sessions(&dir, prepared.settings.retention_days, now_ms());
                let id = self.shared.new_id("s");
                let store = SessionStore::create(&dir, &id, in_project)?;
                let session = Arc::new(Session {
                    id: id.clone(),
                    store,
                    inner: Mutex::new(SessionInner {
                        history: Vec::new(),
                        view: Vec::new(),
                        header: prepared.header.clone(),
                        cleared: false,
                        last_fill: 0,
                        effort: prepared.effort.clone(),
                        seq: 0,
                        running: false,
                        cancel: None,
                        remembered: BTreeSet::new(),
                        tainted: false,
                        usage: Usage::default(),
                        cost_micro: 0,
                        provider: prepared.preset.id.clone(),
                        model: prepared.model.clone(),
                        title: String::new(),
                        created_ms: now_ms(),
                        turns: 0,
                    }),
                });
                self.shared.sessions.lock().insert(id, Arc::clone(&session));
                session
            }
        };
        let cancel = CancellationToken::new();
        {
            let mut inner = session.inner.lock();
            if inner.running {
                return Err(AgentError::new("busy", "this session is still answering"));
            }
            if retry
                && inner
                    .history
                    .last()
                    .is_none_or(|m| m.role != manang_agent_core::Role::User)
            {
                return Err(AgentError::invalid(
                    "there is nothing to retry: send a new message instead",
                ));
            }
            inner.running = true;
            inner.cancel = Some(cancel.clone());
            inner.provider.clone_from(&prepared.preset.id);
            inner.model.clone_from(&prepared.model);
            if inner.effort.is_none() {
                inner.effort.clone_from(&prepared.effort);
            }
            if inner.header.is_empty() && !prepared.header.is_empty() {
                inner.header.clone_from(&prepared.header);
            }
            if inner.title.is_empty() {
                let first = prepared.request_text.trim().lines().next().unwrap_or("");
                inner.title = clip_chars(first, 60).0;
            }
        }
        // What the tools may see: the editor's text, so proposals match what is on screen.
        {
            let mut tc = prepared.tool_ctx.clone();
            tc.session_id.clone_from(&session.id);
            *self.shared.tool_ctx.write() = tc;
        }
        if !retry && let Err(e) = commit(&session, prepared.user_message.clone()) {
            let mut inner = session.inner.lock();
            inner.running = false;
            inner.cancel = None;
            return Err(e);
        }
        write_meta(&session, "running")?;
        let sid = session.id.clone();
        self.shared.emit(
            "agent.started",
            &sid,
            json!({
                "provider": prepared.preset.id, "model": prepared.model,
                "effort": prepared.effort,
                "context_tokens": prepared.context_tokens,
                "window": prepared.window,
                "parts": prepared.parts.iter().map(|c| json!({
                    "id": c.id, "kind": c.kind, "label": c.label, "tokens": c.tokens
                })).collect::<Vec<_>>(),
            }),
        );
        self.shared
            .rt
            .spawn(run(Arc::clone(&self.shared), session, plan, cancel));
        Ok(json!({ "session_id": sid }))
    }

    fn preview(&self, ctx: &Context<'_>, value: Value) -> Result<Value, AgentError> {
        let args: StartArgs = parse(value)?;
        let prepared = self.prepare(ctx, args, false)?;
        let plan = self.plan_for_preview(&prepared);
        let mut view = prepared
            .existing
            .as_ref()
            .map(|s| s.inner.lock().view.clone())
            .unwrap_or_default();
        view.push(prepared.user_message.clone());
        let request = plan.request(&view);
        let spec = prepared.spec.clone();
        let wire_request = request.clone();
        let factory = Arc::clone(&self.shared.factory);
        let wire = self.block_on(async move { factory.preview(&spec, wire_request).await })??;
        let whole = serde_json::to_string(&request).unwrap_or_default();
        let estimate = Usage {
            input: estimate_tokens(&whole),
            ..Usage::default()
        };
        let cost = PriceTable::builtin().estimate(&prepared.model, &estimate);
        Ok(json!({
            "provider": prepared.preset.id,
            "model": prepared.model,
            "url": wire.url,
            "headers": wire.headers,
            "body": wire.body,
            "system": request.system,
            "messages": request.messages,
            "tools": prepared.tools.iter().map(|t| json!({"name": t.name, "risk": t.risk})).collect::<Vec<_>>(),
            "parts": prepared.parts,
            "redaction_count": prepared.parts.iter().map(|c| c.redactions.len()).sum::<usize>()
                + prepared.prompt_redactions as usize,
            "tokens": { "context": prepared.context_tokens, "request": estimate.input },
            "cost": {
                "est_input_micro_usd": cost.map(|c| c.micro_usd),
                "estimate": true,
                "note": "An estimate of the input only; the provider's bill is the truth."
            },
        }))
    }

    /// A plan for building the request only (no network client).
    fn plan_for_preview(&self, p: &Prepared) -> PreviewPlan {
        PreviewPlan {
            model: p.model.clone(),
            system: p.system.clone(),
            tools: p.tools.clone(),
            max_tokens: p.options.max_tokens.unwrap_or(8192).clamp(256, 32_768),
            cache: p.preset.kind == ProviderKind::Anthropic,
        }
    }

    fn cancel(&self, value: Value) -> Result<Value, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            session_id: String,
        }
        let a: A = parse(value)?;
        let session = self
            .shared
            .sessions
            .lock()
            .get(&a.session_id)
            .cloned()
            .ok_or_else(|| AgentError::not_found("no such session"))?;
        let token = session.inner.lock().cancel.clone();
        let cancelled = token.is_some_and(|t| {
            t.cancel();
            true
        });
        Ok(json!({ "cancelled": cancelled }))
    }

    fn approve(&self, value: Value) -> Result<Value, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            call_id: String,
            decision: String,
        }
        let a: A = parse(value)?;
        let decision = Decision::parse(&a.decision).ok_or_else(|| {
            AgentError::invalid("decision must be `allow`, `allow_session` or `deny`")
        })?;
        let pending = self
            .shared
            .approvals
            .lock()
            .remove(&a.call_id)
            .ok_or_else(|| AgentError::not_found("no approval is waiting for that call"))?;
        let _ = pending.tx.send(decision);
        Ok(json!({ "ok": true }))
    }

    fn decide(&self, value: Value, accept: bool) -> Result<Value, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            proposal_id: String,
        }
        let a: A = parse(value)?;
        let proposal = {
            let mut map = self.shared.proposals.lock();
            let p = map
                .get_mut(&a.proposal_id)
                .ok_or_else(|| AgentError::not_found("no such proposal"))?;
            if p.state != ProposalState::Pending {
                return Err(AgentError::new(
                    "already_decided",
                    "this proposal was already accepted or rejected",
                ));
            }
            p.state = if accept {
                ProposalState::Accepted
            } else {
                ProposalState::Rejected
            };
            p.clone()
        };
        proposal.store.append_audit(&AuditEntry {
            ts_ms: now_ms(),
            session: proposal.session_id.clone(),
            event: AuditEvent::Approval {
                tool: format!("proposal:{}", proposal.kind),
                approved: accept,
                by: "user".to_owned(),
            },
        })?;
        if accept {
            Ok(json!({
                "proposal_id": a.proposal_id, "kind": proposal.kind, "edit": proposal.payload
            }))
        } else {
            Ok(json!({ "proposal_id": a.proposal_id, "rejected": true }))
        }
    }

    fn commands_list(&self, value: Value) -> Result<Value, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            #[serde(default)]
            project_root: Option<String>,
        }
        let a: A = parse(value)?;
        let root = self.root_of(a.project_root.as_deref());
        Ok(json!({ "commands": self.slash_commands(root.as_deref()) }))
    }

    fn sessions(&self, value: Value) -> Result<Value, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            #[serde(default)]
            project_root: Option<String>,
        }
        let a: A = parse(value)?;
        let root = self.root_of(a.project_root.as_deref());
        let (dir, _) = self.sessions_dir(root.as_deref());
        Ok(json!({ "sessions": list_sessions(&dir) }))
    }

    fn transcript(&self, value: Value) -> Result<Value, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            session_id: String,
            #[serde(default)]
            project_root: Option<String>,
        }
        let a: A = parse(value)?;
        let root = self.root_of(a.project_root.as_deref());
        let session = self.load_session(&a.session_id, root.as_deref())?;
        let history = session.inner.lock().history.clone();
        Ok(json!({ "session_id": a.session_id, "messages": history }))
    }

    fn models(&self, value: Value) -> Result<Value, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            #[serde(default)]
            provider: Option<String>,
        }
        let a: A = parse(value)?;
        let id = a
            .provider
            .unwrap_or_else(|| self.shared.settings.read().provider.clone());
        let preset = preset_by_id(&id)
            .ok_or_else(|| AgentError::invalid(format!("unknown provider `{id}`")))?;
        Ok(json!({
            "provider": preset.id, "models": preset.models,
            "default": preset.models.first(),
            "fetch_supported": false,
            "note": preset.notes,
        }))
    }

    fn test_connection(&self, value: Value) -> Result<Value, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            #[serde(default)]
            project_root: Option<String>,
        }
        let a: A = parse(value)?;
        let root = self.root_of(a.project_root.as_deref());
        let (settings, policy) = self.policy(root.as_deref())?;
        let fail = |e: &AgentError| {
            Ok(json!({ "ok": false, "code": e.code, "message": friendly(e.code, &e.message) }))
        };
        let (spec, preset) = match self.spec(&settings, &policy, true, None) {
            Ok(v) => v,
            Err(e) => return fail(&e),
        };
        let model = match Self::model_of(&settings, &preset, &Options::default()) {
            Ok(m) => m,
            Err(e) => return fail(&e),
        };
        let provider = match self.shared.factory.build(&spec) {
            Ok(p) => p,
            Err(e) => return fail(&e),
        };
        let request = ChatRequest {
            model: model.clone(),
            system: None,
            messages: vec![Message::user_text("Reply with the single word OK.")?],
            tools: Vec::new(),
            max_tokens: 64,
            cache_breakpoints: Vec::new(),
        };
        let started = Instant::now();
        let outcome = self.block_on(async move {
            let mut stream = provider.stream(&request);
            let work = async {
                while let Some(event) = futures_util::StreamExt::next(&mut stream).await {
                    match event {
                        ProviderEvent::Done { .. } => return None,
                        ProviderEvent::Error { error } => return Some(error),
                        _ => {}
                    }
                }
                None
            };
            tokio::time::timeout(Duration::from_secs(30), work)
                .await
                .unwrap_or_else(|_| {
                    Some(manang_agent_core::ProviderError {
                        code: "timeout".to_owned(),
                        message: "the provider did not answer within 30 seconds".to_owned(),
                        retryable: true,
                        request_id: None,
                    })
                })
        })?;
        match outcome {
            None => Ok(json!({
                "ok": true, "model": model,
                "latency_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
            })),
            Some(e) => Ok(json!({
                "ok": false, "code": e.code, "message": friendly(&e.code, &e.message),
                "request_id": e.request_id
            })),
        }
    }

    fn status(&self, value: Value) -> Result<Value, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            #[serde(default)]
            project_root: Option<String>,
        }
        let a: A = parse(value)?;
        let root = self.root_of(a.project_root.as_deref());
        let settings = self.shared.settings.read().clone();
        let preset = settings.preset()?;
        let mut problems: Vec<&str> = Vec::new();
        let enabled = match self.policy(root.as_deref()) {
            Ok(_) => true,
            Err(e) if e.code == "disabled" => {
                problems.push("disabled");
                false
            }
            Err(_) => {
                problems.push("policy");
                false
            }
        };
        let key = self
            .shared
            .keys
            .handle(
                "agent.key.status",
                Payload::Json(json!({ "provider": preset.id })),
            )
            .ok()
            .and_then(|p| match p {
                Payload::Json(v) => Some(v),
                _ => None,
            })
            .unwrap_or(Value::Null);
        let key_present = key.get("present").and_then(Value::as_bool).unwrap_or(false);
        if preset.key_required && !key_present {
            problems.push("key_missing");
        }
        let consent = self.shared.global.has_consent(&preset.id);
        if !consent {
            problems.push("consent_required");
        }
        let base_url = if settings.base_url.is_empty() {
            preset.base_url.clone().unwrap_or_default()
        } else {
            settings.base_url.clone()
        };
        if settings.offline && check_offline(&base_url).is_err() {
            problems.push("offline_blocked");
        }
        let model = Self::model_of(&settings, &preset, &Options::default()).ok();
        if model.is_none() {
            problems.push("model_missing");
        }
        let host = url::Url::parse(&base_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        let consent_text = if preset.local || is_local_host(&host) {
            format!(
                "When you press Send, Manang sends your message, the text you select and the parts of your project the assistant reads to the server at {host}, which is on this machine. Nothing is sent in the background. You can turn the agent off in Settings."
            )
        } else {
            format!(
                "When you press Send, Manang sends your message, the text you select and the parts of your project the assistant reads (through its read tools) to {} at {host}. Nothing is sent in the background, and you can preview exactly what will be sent. The provider may bill you; your key stays in your system keychain. You can turn the agent off in Settings.",
                preset.display_name
            )
        };
        Ok(json!({
            "enabled": enabled,
            "ready": enabled && problems.is_empty(),
            "problems": problems,
            "provider": preset.id,
            "provider_name": preset.display_name,
            "model": model,
            "base_url": base_url,
            "engine": settings.engine.as_str(),
            "offline": settings.offline,
            "local": preset.local,
            "key_required": preset.key_required,
            "key": key,
            "consent": consent,
            "consent_text": consent_text,
            "context_window": settings.context_window,
            "include_thinking": settings.include_thinking,
            "presets": presets().iter().map(|p| json!({
                "id": p.id, "name": p.display_name, "models": p.models, "base_url": p.base_url,
                "key_required": p.key_required, "local": p.local, "notes": p.notes,
            })).collect::<Vec<_>>(),
        }))
    }

    fn consent(&self, value: Value) -> Result<Value, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            provider: String,
            #[serde(default = "yes")]
            granted: bool,
        }
        fn yes() -> bool {
            true
        }
        let a: A = parse(value)?;
        if preset_by_id(&a.provider).is_none() {
            return Err(AgentError::invalid("unknown provider"));
        }
        self.shared.global.set_consent(&a.provider, a.granted)?;
        Ok(json!({ "provider": a.provider, "consent": a.granted }))
    }

    fn configure(&self, value: Value) -> Result<Value, AgentError> {
        let values = value
            .get("values")
            .and_then(Value::as_object)
            .ok_or_else(|| AgentError::invalid("`values` must be an object"))?;
        let next = self.shared.settings.read().applied(values)?;
        *self.shared.settings.write() = next;
        Ok(json!({ "applied": values.len() }))
    }

    /// The tool context with the running session's pending proposals laid over the editor text.
    fn tool_context(&self) -> ToolContext {
        let mut tc = self.shared.tool_ctx.read().clone();
        tc.pending = self.shared.pending_edits(&tc.session_id);
        tc
    }
}

fn is_local_host(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]"
}

/// The `<project ...>...</project>` header at the start of a stored first message, if any.
fn header_of(history: &[Message]) -> String {
    let text = history.first().map(Message::text).unwrap_or_default();
    match (text.starts_with("<project"), text.find("</project>")) {
        (true, Some(end)) => text[..end + "</project>".len()].to_owned(),
        _ => String::new(),
    }
}

struct PreviewPlan {
    model: String,
    system: String,
    tools: Vec<ToolSpec>,
    max_tokens: u32,
    cache: bool,
}

impl PreviewPlan {
    fn request(&self, view: &[Message]) -> ChatRequest {
        use manang_agent_core::CacheBreakpoint;
        let mut breakpoints = Vec::new();
        if self.cache {
            if !self.tools.is_empty() {
                breakpoints.push(CacheBreakpoint::Tools);
            }
            breakpoints.push(CacheBreakpoint::System);
        }
        ChatRequest {
            model: self.model.clone(),
            system: Some(self.system.clone()),
            messages: view.to_vec(),
            tools: self.tools.clone(),
            max_tokens: self.max_tokens,
            cache_breakpoints: breakpoints,
        }
    }
}

/// Writes `meta.toml` for a session.
pub fn write_meta(session: &Session, state: &str) -> Result<(), AgentError> {
    let meta = {
        let inner = session.inner.lock();
        SessionMeta {
            version: crate::storage::FORMAT_VERSION,
            id: session.id.clone(),
            created_ms: inner.created_ms,
            updated_ms: now_ms(),
            provider: inner.provider.clone(),
            model: inner.model.clone(),
            title: inner.title.clone(),
            state: state.to_owned(),
            turns: inner.turns,
        }
    };
    session.store.write_meta(&meta)
}

fn plain(result: Result<Value, String>) -> Result<Payload, PluginError> {
    result.map(Payload::Json).map_err(PluginError::Failed)
}

impl Plugin for AgentPlugin {
    fn invoke(
        &self,
        ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        if KeyCommands::handles(command) {
            return self.shared.keys.handle(command, input);
        }
        let value = args_of(input)?;
        let out = match command {
            "agent.start" => self.start(ctx, value),
            "agent.cancel" => self.cancel(value),
            "agent.approve" => self.approve(value),
            "agent.apply-proposal" => self.decide(value, true),
            "agent.reject-proposal" => self.decide(value, false),
            "agent.preview-request" => self.preview(ctx, value),
            "agent.commands" => self.commands_list(value),
            "agent.sessions" => self.sessions(value),
            "agent.transcript" => self.transcript(value),
            "agent.models" => self.models(value),
            "agent.test-connection" => self.test_connection(value),
            "agent.status" => self.status(value),
            "agent.consent" => self.consent(value),
            "agent.configure" => self.configure(value),
            "agent.list-files" => {
                return plain(tools::list_files(ctx, &self.tool_context(), &value));
            }
            "agent.read-file" => return plain(tools::read_file(ctx, &self.tool_context(), &value)),
            "agent.search" => return plain(tools::search(ctx, &self.tool_context(), &value)),
            "agent.outline" => {
                return plain(tools::outline_tool(ctx, &self.tool_context(), &value));
            }
            "agent.diagnostics" => {
                return plain(tools::diagnostics(ctx, &self.tool_context(), &value));
            }
            "agent.bib-search" => return plain(tools::bib_search(ctx, &value)),
            "agent.edit-text" => return plain(tools::edit_text(ctx, &self.tool_context(), &value)),
            "agent.ask-user" => return plain(tools::ask_user(&value)),
            other => return Err(PluginError::UnknownCommand(other.to_owned())),
        }?;
        Ok(Payload::Json(out))
    }

    fn on_hook(
        &self,
        _ctx: &Context<'_>,
        hook: Hook,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        match hook {
            Hook::RegisterAgentTool => Ok(Payload::Json(tools::declarations())),
            _ => Ok(input),
        }
    }
}
