//! The `agent.key.*` commands.
//!
//! * `agent.key.set {provider, key, account?}` -> `{stored, saved, notice?}`
//! * `agent.key.status {provider, account?}` -> `{present, store, saved, notice?}`
//! * `agent.key.clear {provider, account?}` -> `{cleared}`
//!
//! None of them ever returns the key, and no error text contains it.
//!
//! Registration note for the orchestrator (WP4): command ids must start with
//! the owning plugin's name, so these three belong to a plugin named `agent`,
//! as does the orchestrator. Either the orchestrator's manifest lists these
//! commands and delegates to [`KeyCommands::handle`], or this crate's
//! standalone `manifest()`/`plugin()` is registered under that name and the
//! orchestrator's commands are added to it. They cannot be two plugins.

use crate::chain::ChainKeyStore;
use crate::error::KeystoreError;
use crate::store::{KeyId, StoreKind, clean_key};
use manang_kernel::{Context, Manifest, ManifestError, Payload, Plugin, PluginError};
use serde_json::{Map, Value, json};
use std::sync::Arc;

const MANIFEST: &str = include_str!("../manifest.toml");

/// The ids this crate handles.
pub const COMMANDS: [&str; 3] = ["agent.key.set", "agent.key.status", "agent.key.clear"];

const SESSION_NOTICE: &str =
    "Not saved: no system keychain is available, so the key is kept in memory until Manang closes.";
const KERNEL_NOTICE: &str =
    "Saved in the Linux kernel keyring: it survives restarts of Manang but not a reboot.";

/// The standalone manifest (plugin `agent`, the three key commands).
pub fn manifest() -> Result<Manifest, ManifestError> {
    Manifest::from_toml(MANIFEST)
}

/// The standalone plugin over the best store for this machine.
pub fn plugin() -> impl Plugin {
    KeystorePlugin::new(KeyCommands::new(Arc::new(crate::detect())))
}

/// Executes the key commands over a store chain.
#[derive(Clone)]
pub struct KeyCommands {
    store: Arc<ChainKeyStore>,
}

impl std::fmt::Debug for KeyCommands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyCommands")
            .field("store", &self.store)
            .finish()
    }
}

fn fail(error: &KeystoreError) -> PluginError {
    PluginError::Failed(error.to_string())
}

fn bad(message: &str) -> PluginError {
    PluginError::Failed(message.to_owned())
}

fn describe(kind: StoreKind) -> Value {
    let mut out = Map::new();
    out.insert("store".to_owned(), json!(kind.as_str()));
    out.insert("saved".to_owned(), json!(kind.is_saved()));
    match kind {
        StoreKind::Session => {
            out.insert("notice".to_owned(), json!(SESSION_NOTICE));
        }
        StoreKind::Kernel => {
            out.insert("notice".to_owned(), json!(KERNEL_NOTICE));
        }
        StoreKind::Keychain => {}
    }
    Value::Object(out)
}

impl KeyCommands {
    pub fn new(store: Arc<ChainKeyStore>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &Arc<ChainKeyStore> {
        &self.store
    }

    /// Whether `command` is one of ours.
    pub fn handles(command: &str) -> bool {
        COMMANDS.contains(&command)
    }

    /// Runs one command.
    pub fn handle(&self, command: &str, input: Payload) -> Result<Payload, PluginError> {
        let args = match input {
            Payload::Json(Value::Object(map)) => map,
            _ => return Err(bad("expected a JSON object with named arguments")),
        };
        let allowed: &[&str] = if command == "agent.key.set" {
            &["provider", "key", "account"]
        } else {
            &["provider", "account"]
        };
        if let Some(unknown) = args.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(PluginError::Failed(format!(
                "unexpected argument `{unknown}`"
            )));
        }
        let text = |name: &str| -> Result<Option<&str>, PluginError> {
            match args.get(name) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(s)) => Ok(Some(s.as_str())),
                Some(_) => Err(PluginError::Failed(format!("`{name}` must be a string"))),
            }
        };
        let provider = text("provider")?.ok_or_else(|| bad("`provider` is required"))?;
        let account = text("account")?.unwrap_or(KeyId::DEFAULT_ACCOUNT);
        let id = KeyId::new(provider, account).map_err(|e| fail(&e))?;

        let out = match command {
            "agent.key.set" => {
                let raw = text("key")?.ok_or_else(|| bad("`key` is required"))?;
                let key = clean_key(raw).map_err(|e| fail(&e))?;
                let kind = self.store.set(&id, &key).map_err(|e| fail(&e))?;
                describe(kind)
            }
            "agent.key.status" => {
                let status = self.store.status(&id).map_err(|e| fail(&e))?;
                let mut out = describe(status.store);
                if let Value::Object(map) = &mut out {
                    map.insert("present".to_owned(), json!(status.present));
                    if !status.present {
                        // Not stored yet: `saved` and `notice` describe where it would go.
                        map.insert("saved".to_owned(), json!(status.store.is_saved()));
                    }
                }
                out
            }
            "agent.key.clear" => {
                let cleared = self.store.delete(&id).map_err(|e| fail(&e))?;
                json!({ "cleared": cleared })
            }
            other => return Err(PluginError::UnknownCommand(other.to_owned())),
        };
        Ok(Payload::Json(out))
    }
}

/// [`KeyCommands`] as a kernel plugin.
#[derive(Debug, Clone)]
pub struct KeystorePlugin {
    commands: KeyCommands,
}

impl KeystorePlugin {
    pub fn new(commands: KeyCommands) -> Self {
        Self { commands }
    }
}

impl Plugin for KeystorePlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        self.commands.handle(command, input)
    }
}
