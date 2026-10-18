//! Where the agent keeps things.
//!
//! * `<project>/.manang/agent/sessions/<id>/{transcript.jsonl, audit.jsonl, meta.toml}`, with a
//!   generated `.manang/agent/.gitignore` of `*` (the user's own `.gitignore` is never edited);
//!   without a project the same layout lives under `<config>/agent/`.
//! * `<project>/.manang/agent/policy.toml`: a policy layer that may only tighten.
//! * `<config>/agent/consent.json` and `<config>/agent/usage.jsonl`: global, never a key.
//!
//! Every format carries `version = 1`.

use crate::error::AgentError;
use manang_agent_core::{AuditEntry, PolicyLayer, TranscriptEntry, append, read_all};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const FORMAT_VERSION: u32 = 1;
const GITIGNORE: &str = "*\n";

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn io(what: &str, error: &std::io::Error) -> AgentError {
    AgentError::storage(format!("{what}: {error}"))
}

/// The agent folder of a project: `<root>/.manang/agent`.
pub fn project_agent_dir(root: &Path) -> PathBuf {
    root.join(".manang").join("agent")
}

/// Creates `dir` (and the project's `.gitignore` of `*` beside it when it is a project folder).
fn ensure_dir(dir: &Path, with_gitignore: bool) -> Result<(), AgentError> {
    fs::create_dir_all(dir).map_err(|e| io("creating the agent folder", &e))?;
    if with_gitignore {
        // `dir` is `<root>/.manang/agent/sessions/<id>`: find the `agent` folder above it.
        let agent = dir
            .ancestors()
            .find(|p| p.file_name().is_some_and(|n| n == "agent"));
        if let Some(agent) = agent {
            let ignore = agent.join(".gitignore");
            if !ignore.exists() {
                fs::write(&ignore, GITIGNORE).map_err(|e| io("writing .gitignore", &e))?;
            }
        }
    }
    Ok(())
}

/// What `meta.toml` holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMeta {
    pub version: u32,
    pub id: String,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub provider: String,
    pub model: String,
    pub title: String,
    /// `idle`, `running` or `ended`.
    pub state: String,
    #[serde(default)]
    pub turns: u32,
}

/// One session's folder.
#[derive(Debug, Clone)]
pub struct SessionStore {
    dir: PathBuf,
}

impl SessionStore {
    /// `sessions_dir` is `<...>/agent/sessions`; `project` says whether to place the gitignore.
    pub fn create(sessions_dir: &Path, id: &str, project: bool) -> Result<Self, AgentError> {
        let dir = sessions_dir.join(id);
        ensure_dir(&dir, project)?;
        Ok(Self { dir })
    }

    pub fn open(sessions_dir: &Path, id: &str) -> Option<Self> {
        let dir = sessions_dir.join(id);
        dir.is_dir().then_some(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn open_append(&self, name: &str) -> Result<fs::File, AgentError> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(name))
            .map_err(|e| io(&format!("opening {name}"), &e))
    }

    pub fn append_transcript(&self, entry: &TranscriptEntry) -> Result<(), AgentError> {
        let mut file = self.open_append("transcript.jsonl")?;
        append(&mut file, entry)?;
        file.flush().map_err(|e| io("writing transcript", &e))
    }

    pub fn append_audit(&self, entry: &AuditEntry) -> Result<(), AgentError> {
        let mut file = self.open_append("audit.jsonl")?;
        append(&mut file, entry)?;
        file.flush().map_err(|e| io("writing audit log", &e))
    }

    pub fn write_meta(&self, meta: &SessionMeta) -> Result<(), AgentError> {
        let text = toml::to_string(meta).map_err(|e| AgentError::storage(e.to_string()))?;
        // Write beside, then rename: a reader never sees a half-written file.
        let tmp = self.dir.join("meta.toml.tmp");
        fs::write(&tmp, text).map_err(|e| io("writing meta.toml", &e))?;
        fs::rename(&tmp, self.dir.join("meta.toml")).map_err(|e| io("writing meta.toml", &e))
    }

    pub fn read_meta(&self) -> Option<SessionMeta> {
        let text = fs::read_to_string(self.dir.join("meta.toml")).ok()?;
        toml::from_str(&text).ok()
    }

    pub fn read_transcript(&self) -> Result<Vec<TranscriptEntry>, AgentError> {
        let path = self.dir.join("transcript.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(&path).map_err(|e| io("reading transcript", &e))?;
        Ok(read_all::<TranscriptEntry>(&text)?.entries)
    }

    pub fn read_audit(&self) -> Result<Vec<AuditEntry>, AgentError> {
        let path = self.dir.join("audit.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(&path).map_err(|e| io("reading audit log", &e))?;
        Ok(read_all::<AuditEntry>(&text)?.entries)
    }
}

/// Session metadata in `sessions_dir`, newest first. Unreadable folders are skipped.
pub fn list_sessions(sessions_dir: &Path) -> Vec<SessionMeta> {
    let Ok(entries) = fs::read_dir(sessions_dir) else {
        return Vec::new();
    };
    let mut out: Vec<SessionMeta> = entries
        .flatten()
        .filter_map(|e| SessionStore { dir: e.path() }.read_meta())
        .collect();
    out.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms).then(a.id.cmp(&b.id)));
    out
}

/// Deletes sessions not updated for `days` days (0 keeps everything). Returns how many went.
pub fn prune_sessions(sessions_dir: &Path, days: u32, now: u64) -> usize {
    if days == 0 {
        return 0;
    }
    let limit = u64::from(days) * 86_400_000;
    let mut removed = 0;
    for meta in list_sessions(sessions_dir) {
        if meta.state != "running"
            && now.saturating_sub(meta.updated_ms) > limit
            && fs::remove_dir_all(sessions_dir.join(&meta.id)).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

/// The project's policy layer, if it has one. An unreadable or invalid file is an error: the
/// caller must fail closed, because a file meant to tighten the policy must not be ignored.
pub fn read_project_policy(root: &Path) -> Result<Option<PolicyLayer>, AgentError> {
    let path = project_agent_dir(root).join("policy.toml");
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| {
        AgentError::new(
            "disabled",
            format!("the project's AI policy could not be read ({e}); the agent stays off"),
        )
    })?;
    let layer = toml::from_str::<PolicyLayer>(&text).map_err(|e| {
        AgentError::new(
            "disabled",
            format!(
                "the project's AI policy (.manang/agent/policy.toml) is invalid ({}); the agent stays off",
                e.message()
            ),
        )
    })?;
    Ok(Some(layer))
}

/// Per-provider consent, stored in the user's config folder.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ConsentFile {
    version: u32,
    #[serde(default)]
    providers: BTreeMap<String, u64>,
}

/// Global files: consent and the usage ledger.
#[derive(Debug, Clone)]
pub struct GlobalStore {
    dir: PathBuf,
}

impl GlobalStore {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            dir: config_dir.join("agent"),
        }
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.dir.join("sessions")
    }

    fn consent_path(&self) -> PathBuf {
        self.dir.join("consent.json")
    }

    fn read_consent(&self) -> ConsentFile {
        fs::read_to_string(self.consent_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn has_consent(&self, provider: &str) -> bool {
        self.read_consent().providers.contains_key(provider)
    }

    pub fn set_consent(&self, provider: &str, granted: bool) -> Result<(), AgentError> {
        let mut file = self.read_consent();
        file.version = FORMAT_VERSION;
        if granted {
            file.providers.insert(provider.to_owned(), now_ms());
        } else {
            file.providers.remove(provider);
        }
        fs::create_dir_all(&self.dir).map_err(|e| io("creating the agent folder", &e))?;
        let text =
            serde_json::to_string_pretty(&file).map_err(|e| AgentError::storage(e.to_string()))?;
        fs::write(self.consent_path(), text).map_err(|e| io("saving consent", &e))
    }

    /// Appends one usage line to the global ledger (no key, no text).
    pub fn append_usage(&self, line: &serde_json::Value) -> Result<(), AgentError> {
        fs::create_dir_all(&self.dir).map_err(|e| io("creating the agent folder", &e))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("usage.jsonl"))
            .map_err(|e| io("opening the usage ledger", &e))?;
        append(&mut file, line)?;
        file.flush().map_err(|e| io("writing the usage ledger", &e))
    }
}
