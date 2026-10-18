//! State shared by the commands and the session tasks.

use crate::error::AgentError;
use crate::prompt::Feature;
use crate::provider::ProviderFactory;
use crate::risk::{Decision, RiskTable};
use crate::settings::Settings;
use crate::storage::{GlobalStore, SessionStore, now_ms};
use crate::tools::ToolContext;
use manang_agent_core::{Message, Usage};
use manang_kernel::{EventBus, Kernel, Payload};
use manang_keystore::KeyCommands;
use parking_lot::{Mutex, RwLock};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Time limits of a session. The defaults are for people; tests shorten them.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// No event from the provider for this long ends the turn with `timeout`.
    pub idle: Duration,
    /// One tool call.
    pub tool: Duration,
    /// An approval nobody answers is denied after this.
    pub approval: Duration,
    /// Streaming deltas are sent to the UI at most this often.
    pub flush: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            idle: Duration::from_secs(120),
            tool: Duration::from_secs(60),
            approval: Duration::from_secs(300),
            flush: Duration::from_millis(30),
        }
    }
}

/// What the orchestrator keeps per session.
#[derive(Debug)]
pub struct SessionInner {
    pub history: Vec<Message>,
    pub seq: u64,
    pub running: bool,
    pub cancel: Option<CancellationToken>,
    /// Approval scopes the user allowed for the rest of the session.
    pub remembered: BTreeSet<String>,
    /// The session has seen untrusted external content (a network tool's output).
    pub tainted: bool,
    pub usage: Usage,
    pub cost_micro: u64,
    pub feature: Feature,
    pub provider: String,
    pub model: String,
    pub title: String,
    pub created_ms: u64,
    pub turns: u32,
}

/// One conversation.
#[derive(Debug)]
pub struct Session {
    pub id: String,
    pub store: SessionStore,
    pub inner: Mutex<SessionInner>,
}

/// A proposal waiting for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    Pending,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone)]
pub struct StoredProposal {
    pub session_id: String,
    pub store: SessionStore,
    pub kind: String,
    pub payload: Value,
    pub state: ProposalState,
}

pub struct PendingApproval {
    pub tx: tokio::sync::oneshot::Sender<Decision>,
    pub session_id: String,
}

/// What [`Shared::new`] needs.
pub struct SharedInit {
    pub events: EventBus,
    pub global: GlobalStore,
    pub keys: Arc<KeyCommands>,
    pub factory: Arc<dyn ProviderFactory>,
    pub risk: RiskTable,
    pub limits: Limits,
    pub rt: tokio::runtime::Handle,
    pub pool: rayon::ThreadPool,
}

/// Shared by the plugin and its tasks.
pub struct Shared {
    pub events: EventBus,
    pub global: GlobalStore,
    pub keys: Arc<KeyCommands>,
    pub factory: Arc<dyn ProviderFactory>,
    pub risk: RiskTable,
    pub limits: Limits,
    pub settings: RwLock<Settings>,
    pub kernel: Mutex<Weak<Kernel>>,
    pub rt: tokio::runtime::Handle,
    pub pool: rayon::ThreadPool,
    pub sessions: Mutex<HashMap<String, Arc<Session>>>,
    pub approvals: Mutex<HashMap<String, PendingApproval>>,
    pub proposals: Mutex<HashMap<String, StoredProposal>>,
    pub tool_ctx: RwLock<ToolContext>,
    pub last_project: Mutex<Option<PathBuf>>,
    counter: AtomicU64,
}

impl Shared {
    pub fn new(init: SharedInit) -> Self {
        Self {
            events: init.events,
            global: init.global,
            keys: init.keys,
            factory: init.factory,
            risk: init.risk,
            limits: init.limits,
            settings: RwLock::new(Settings::default()),
            kernel: Mutex::new(Weak::new()),
            rt: init.rt,
            pool: init.pool,
            sessions: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
            proposals: Mutex::new(HashMap::new()),
            tool_ctx: RwLock::new(ToolContext::default()),
            last_project: Mutex::new(None),
            counter: AtomicU64::new(1),
        }
    }

    /// A fresh id of lowercase hex, safe as a folder name.
    pub fn new_id(&self, prefix: &str) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}{:x}-{n:x}", now_ms())
    }

    /// Emits an agent event; `session_id` is added.
    pub fn emit(&self, topic: &str, session_id: &str, fields: Value) {
        let mut map = match fields {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        map.insert("session_id".to_owned(), Value::String(session_id.to_owned()));
        self.events.emit(topic, Payload::Json(Value::Object(map)));
    }

    pub fn kernel(&self) -> Result<Arc<Kernel>, AgentError> {
        self.kernel
            .lock()
            .upgrade()
            .ok_or_else(|| AgentError::unavailable("the agent is not connected to the kernel"))
    }
}

/// Session ids come from outside: only `[a-z0-9_-]` and a bounded length, so one can never name a
/// path.
pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}
