//! The `agent.*` commands.

use crate::context::{
    ChipInput, ChipKind, ResolvedChip, check_total, diagnostics_text, render_user_message,
    resolve_chip,
};
use crate::error::AgentError;
use crate::prompt::{Feature, system_prompt};
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
use crate::text::{estimate_tokens, outline};
use crate::tools::{Selection, ToolContext};
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
use std::collections::BTreeSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const MANIFEST: &str = include_str!("../manifest.toml");
const MAX_PROMPT_CHARS: usize = 20_000;
const MAX_RUNNING: usize = 4;

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
    max_total_tokens: Option<u64>,
    target_language: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartArgs {
    #[serde(default)]
    session_id: Option<String>,
    feature: Feature,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    context: Vec<ChipInput>,
    #[serde(default)]
    options: Options,
    #[serde(default)]
    project_root: Option<String>,
}

struct Prepared {
    settings: Settings,
    preset: ProviderPreset,
    spec: ProviderSpec,
    model: String,
    chips: Vec<ResolvedChip>,
    prompt_redactions: u32,
    system: String,
    tools: Vec<ToolSpec>,
    names: ToolNameMap,
    user_message: Message,
    existing: Option<Arc<Session>>,
    project_root: Option<PathBuf>,
    options: Options,
    feature: Feature,
    redaction: RedactionConfig,
    context_tokens: u64,
    diagnostics: Vec<crate::context::Diagnostic>,
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

    fn chip_text(
        &self,
        ctx: &Context<'_>,
        chip: &ChipInput,
        all: &[ChipInput],
    ) -> Result<String, AgentError> {
        if let Some(text) = &chip.text {
            return Ok(text.clone());
        }
        match chip.kind {
            ChipKind::Diagnostics => Ok(diagnostics_text(&chip.diagnostics)),
            ChipKind::Figure => {
                let args = chip
                    .path
                    .as_ref()
                    .map_or(json!({}), |p| json!({ "path": p }));
                let out = ctx
                    .invoke("figure.describe", Payload::Json(args))
                    .map_err(|e| {
                        AgentError::unavailable(format!("the figure could not be described: {e}"))
                    })?;
                let Payload::Json(v) = out else {
                    return Err(AgentError::unavailable("the figure has no description"));
                };
                Ok(v.get("summary")
                    .and_then(Value::as_str)
                    .map_or_else(|| v.to_string(), str::to_owned))
            }
            ChipKind::Outline => {
                let path = chip
                    .path
                    .as_deref()
                    .ok_or_else(|| AgentError::invalid("an outline chip needs a path"))?;
                let known = all
                    .iter()
                    .find(|c| c.kind == ChipKind::File && c.path.as_deref() == Some(path))
                    .and_then(|c| c.text.clone());
                let text = match known {
                    Some(t) => t,
                    None => match ctx
                        .invoke("project.read-file", Payload::Json(json!({ "path": path })))
                    {
                        Ok(Payload::Json(Value::String(t))) => t,
                        _ => {
                            return Err(AgentError::not_found(format!("{path} could not be read")));
                        }
                    },
                };
                Ok(outline(path, &text)
                    .iter()
                    .map(|h| {
                        format!(
                            "{}{} (line {})",
                            "  ".repeat(usize::from(h.level) - 1),
                            h.title,
                            h.line
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
            ChipKind::BibliographyKeys => {
                let out = ctx
                    .invoke(
                        "citations.search",
                        Payload::Json(json!({ "query": "", "limit": 200 })),
                    )
                    .map_err(|e| {
                        AgentError::unavailable(format!("the bibliography could not be read: {e}"))
                    })?;
                let Payload::Json(v) = out else {
                    return Ok(String::new());
                };
                let keys: Vec<&str> = v
                    .get("results")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|r| r.get("key").and_then(Value::as_str))
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(keys.join(", "))
            }
            kind => Err(AgentError::invalid(format!(
                "the {} chip has no text",
                kind.as_str()
            ))),
        }
    }

    fn prepare(
        &self,
        ctx: &Context<'_>,
        args: StartArgs,
        sending: bool,
    ) -> Result<Prepared, AgentError> {
        let root = self.root_of(args.project_root.as_deref());
        let (settings, policy) = self.policy(root.as_deref())?;
        let (spec, preset) = self.spec(&settings, &policy, sending)?;
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
        let prompt = args.prompt.clone().unwrap_or_default();
        if prompt.chars().count() > MAX_PROMPT_CHARS {
            return Err(AgentError::invalid("the message is too long"));
        }
        let redaction = RedactionConfig {
            secrets: true,
            emails: settings.redact_emails,
            paths: settings.redact_paths,
        };
        let mut chips = Vec::with_capacity(args.context.len());
        for (i, chip) in args.context.iter().enumerate() {
            let text = self.chip_text(ctx, chip, &args.context)?;
            chips.push(resolve_chip(chip, i, &text, &redaction));
        }
        let request_text = if prompt.trim().is_empty() {
            args.feature
                .default_request(args.options.target_language.as_deref())
        } else {
            prompt
        };
        let request_redaction = redact(&request_text, &redaction);
        let limit = policy
            .max_context_tokens
            .unwrap_or(settings.max_context_tokens);
        let context_tokens = check_total(&chips, estimate_tokens(&request_redaction.text), limit)?;
        let user_text = render_user_message(&chips, &request_redaction.text);
        let user_message = Message::user_text(&user_text)?;

        let kernel = self.shared.kernel()?;
        let has_figure = chips.iter().any(|c| c.kind == ChipKind::Figure);
        let mut tools = Vec::new();
        for registered in kernel.agent_tools().tools {
            let t = registered.tool;
            let Some(info) = self.shared.risk.get(&t.name) else {
                continue;
            };
            if !args.feature.offers(&t.name, has_figure) {
                continue;
            }
            tools.push(ToolSpec {
                name: t.name,
                title: Some(t.title),
                description: t.description,
                input_schema: t.input_schema,
                output_schema: Some(t.output_schema),
                risk: info.risk,
            });
        }
        let names = ToolNameMap::new(tools.iter().map(|t| t.name.as_str()))?;
        let existing = match &args.session_id {
            Some(id) => Some(self.load_session(id, root.as_deref())?),
            None => None,
        };
        Ok(Prepared {
            system: system_prompt(args.feature, args.options.target_language.as_deref()),
            prompt_redactions: u32::try_from(request_redaction.spans.len()).unwrap_or(u32::MAX),
            settings,
            preset,
            spec,
            model,
            chips,
            tools,
            names,
            user_message,
            existing,
            project_root: root,
            feature: args.feature,
            options: args.options,
            redaction,
            context_tokens,
            diagnostics: args
                .context
                .iter()
                .flat_map(|c| c.diagnostics.clone())
                .collect(),
        })
    }

    fn plan(&self, p: &Prepared) -> Result<TurnPlan, AgentError> {
        let provider = self.shared.factory.build(&p.spec)?;
        let redaction_count: usize = p.chips.iter().map(|c| c.redactions.len()).sum::<usize>()
            + p.prompt_redactions as usize;
        Ok(TurnPlan {
            provider,
            provider_id: p.preset.id.clone(),
            model: p.model.clone(),
            system: p.system.clone(),
            tools: p.tools.clone(),
            names: p.names.clone(),
            max_tokens: p.options.max_tokens.unwrap_or(8192).clamp(256, 32_768),
            max_iterations: p.options.max_iterations.unwrap_or(8).clamp(1, 16),
            max_total_tokens: p.options.max_total_tokens.unwrap_or(400_000),
            include_thinking: p.settings.include_thinking,
            cache: p.preset.kind == ProviderKind::Anthropic,
            redaction: p.redaction,
            redaction_count: u32::try_from(redaction_count).unwrap_or(u32::MAX),
            prices: PriceTable::builtin(),
        })
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
        let session = Arc::new(Session {
            id: id.to_owned(),
            inner: Mutex::new(SessionInner {
                seq: entries.last().map_or(0, |e| e.seq),
                history: entries.into_iter().map(|e| e.message).collect(),
                running: false,
                cancel: None,
                remembered: BTreeSet::new(),
                tainted: false,
                usage: Usage::default(),
                cost_micro: 0,
                feature: Feature::Chat,
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
                        seq: 0,
                        running: false,
                        cancel: None,
                        remembered: BTreeSet::new(),
                        tainted: false,
                        usage: Usage::default(),
                        cost_micro: 0,
                        feature: prepared.feature,
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
            inner.running = true;
            inner.cancel = Some(cancel.clone());
            inner.feature = prepared.feature;
            inner.provider.clone_from(&prepared.preset.id);
            inner.model.clone_from(&prepared.model);
            if inner.title.is_empty() {
                let text = prepared.user_message.text();
                let first = text
                    .split("<user_request>")
                    .nth(1)
                    .unwrap_or(&text)
                    .trim()
                    .lines()
                    .next()
                    .unwrap_or("");
                inner.title = crate::text::clip_chars(first, 60).0;
            }
        }
        // What the tools may see: the text the user attached, so proposals match the editor.
        {
            let mut tc = ToolContext {
                project_root: prepared.project_root.clone(),
                strict_markup: prepared.feature.strict_markup(),
                ..ToolContext::default()
            };
            for chip in &prepared.chips {
                match (chip.kind, &chip.path) {
                    (ChipKind::File, Some(path)) => {
                        tc.files.insert(path.clone(), chip.source.clone());
                    }
                    (ChipKind::Selection, Some(path)) => {
                        if let Some([from, to]) = chip.range {
                            tc.selections.push(Selection {
                                path: path.clone(),
                                from,
                                to,
                                text: chip.source.clone(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            tc.diagnostics.clone_from(&prepared.diagnostics);
            *self.shared.tool_ctx.write() = tc;
        }
        if let Err(e) = commit(&session, prepared.user_message.clone()) {
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
                "feature": prepared.feature.as_str(),
                "context_tokens": prepared.context_tokens,
                "chips": prepared.chips.iter().map(|c| json!({
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
        let mut history = prepared
            .existing
            .as_ref()
            .map(|s| s.inner.lock().history.clone())
            .unwrap_or_default();
        history.push(prepared.user_message.clone());
        let request = plan.request(&history);
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
            "chips": prepared.chips,
            "redaction_count": prepared.chips.iter().map(|c| c.redactions.len()).sum::<usize>()
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
        let (spec, preset) = match self.spec(&settings, &policy, true) {
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
                "When you press Send, Manang sends the text you attach as context and your \
                 message to the server at {host}, which is on this machine. Nothing is sent in \
                 the background. You can turn the agent off in Settings."
            )
        } else {
            format!(
                "When you press Send, Manang sends the text you attach as context (the selection, \
                 file, problems or figure you choose in the context tray) and your message to \
                 {} at {host}. Nothing is sent in the background, and you can preview exactly \
                 what will be sent. The provider may bill you; your key stays in your system \
                 keychain. You can turn the agent off in Settings.",
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
            "max_context_tokens": settings.max_context_tokens,
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
}

fn is_local_host(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]"
}

struct PreviewPlan {
    model: String,
    system: String,
    tools: Vec<ToolSpec>,
    max_tokens: u32,
    cache: bool,
}

impl PreviewPlan {
    fn request(&self, history: &[Message]) -> ChatRequest {
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
            messages: history.to_vec(),
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
            feature: inner.feature.as_str().to_owned(),
            provider: inner.provider.clone(),
            model: inner.model.clone(),
            title: inner.title.clone(),
            state: state.to_owned(),
            turns: inner.turns,
        }
    };
    session.store.write_meta(&meta)
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
            "agent.sessions" => self.sessions(value),
            "agent.transcript" => self.transcript(value),
            "agent.models" => self.models(value),
            "agent.test-connection" => self.test_connection(value),
            "agent.status" => self.status(value),
            "agent.consent" => self.consent(value),
            "agent.configure" => self.configure(value),
            "agent.doc-read" => {
                let tc = self.shared.tool_ctx.read().clone();
                crate::tools::doc_read(ctx, &tc, &value)
            }
            "agent.doc-diagnostics" => {
                let tc = self.shared.tool_ctx.read().clone();
                crate::tools::doc_diagnostics(&tc, &value)
            }
            "agent.doc-outline" => {
                let tc = self.shared.tool_ctx.read().clone();
                crate::tools::doc_outline(ctx, &tc, &value)
            }
            "agent.doc-propose-edit" => {
                let tc = self.shared.tool_ctx.read().clone();
                crate::tools::doc_propose_edit(ctx, &tc, &value)
            }
            "agent.project-search" => {
                let tc = self.shared.tool_ctx.read().clone();
                crate::tools::project_search(ctx, &tc, &value)
            }
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
            Hook::RegisterAgentTool => Ok(Payload::Json(crate::tools::declarations())),
            _ => Ok(input),
        }
    }
}
