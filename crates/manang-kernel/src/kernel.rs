//! The registry and dispatcher that hosts plugins.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::Serialize;

use crate::capability::CapabilitySet;
use crate::error::KernelError;
use crate::manifest::{Hook, HookMode, Manifest};
use crate::payload::Payload;
use crate::plugin::{Context, Plugin};

struct Entry {
    manifest: Manifest,
    capabilities: CapabilitySet,
    plugin: Arc<dyn Plugin>,
}

#[derive(Default)]
struct Registry {
    /// Registration order, which defines hook pipeline order.
    order: Vec<String>,
    plugins: HashMap<String, Arc<Entry>>,
    /// command id -> owning plugin name.
    commands: HashMap<String, String>,
}

/// A command as shown in the command palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandInfo {
    pub id: String,
    pub title: String,
    pub plugin: String,
}

/// Hosts plugins and routes commands and hooks to them.
#[derive(Default)]
pub struct Kernel {
    registry: RwLock<Registry>,
}

impl Kernel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin. Atomic: on error nothing is registered.
    pub fn register(&self, manifest: Manifest, plugin: Arc<dyn Plugin>) -> Result<(), KernelError> {
        manifest.validate()?;
        let capabilities = manifest.capabilities()?;
        let name = manifest.plugin.name.clone();

        let mut reg = self.registry.write();
        if reg.plugins.contains_key(&name) {
            return Err(KernelError::DuplicatePlugin(name));
        }
        for command in &manifest.contributes.commands {
            if let Some(owner) = reg.commands.get(&command.id) {
                return Err(KernelError::CommandTaken {
                    command: command.id.clone(),
                    owner: owner.clone(),
                });
            }
        }
        for command in &manifest.contributes.commands {
            reg.commands.insert(command.id.clone(), name.clone());
        }
        reg.order.push(name.clone());
        reg.plugins.insert(
            name,
            Arc::new(Entry {
                manifest,
                capabilities,
                plugin,
            }),
        );
        Ok(())
    }

    /// Remove a plugin and everything it contributed.
    pub fn unregister(&self, name: &str) -> Result<(), KernelError> {
        let mut reg = self.registry.write();
        let entry = reg
            .plugins
            .remove(name)
            .ok_or_else(|| KernelError::UnknownPlugin(name.to_owned()))?;
        for command in &entry.manifest.contributes.commands {
            reg.commands.remove(&command.id);
        }
        reg.order.retain(|n| n != name);
        Ok(())
    }

    /// Every registered command, sorted by id, for the command palette.
    pub fn commands(&self) -> Vec<CommandInfo> {
        let reg = self.registry.read();
        let mut out: Vec<CommandInfo> = reg
            .plugins
            .values()
            .flat_map(|e| {
                e.manifest.contributes.commands.iter().map(|c| CommandInfo {
                    id: c.id.clone(),
                    title: c.title.clone(),
                    plugin: e.manifest.plugin.name.clone(),
                })
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// Run a command. No kernel lock is held while the plugin executes.
    pub fn invoke(&self, command: &str, input: Payload) -> Result<Payload, KernelError> {
        let entry = {
            let reg = self.registry.read();
            let owner = reg
                .commands
                .get(command)
                .ok_or_else(|| KernelError::UnknownCommand(command.to_owned()))?;
            reg.plugins
                .get(owner)
                .cloned()
                .ok_or_else(|| KernelError::UnknownPlugin(owner.clone()))?
        };
        let ctx = Context::new(&entry.manifest.plugin.name, &entry.capabilities);
        Ok(entry.plugin.invoke(&ctx, command, input)?)
    }

    fn subscribers(&self, hook: Hook) -> Vec<Arc<Entry>> {
        let reg = self.registry.read();
        reg.order
            .iter()
            .filter_map(|n| reg.plugins.get(n))
            .filter(|e| e.manifest.contributes.hooks.contains(&hook))
            .cloned()
            .collect()
    }

    /// Dispatch a hook to its subscribers.
    ///
    /// [`HookMode::Pipeline`] hooks return one payload (the last output);
    /// [`HookMode::FanOut`] hooks return one `(plugin, payload)` per
    /// subscriber. Either way the result is a list of `(plugin, payload)`:
    /// pipelines yield at most one entry. The first failing subscriber aborts
    /// a pipeline; a fan-out reports each subscriber's own result.
    pub fn run_hook(
        &self,
        hook: Hook,
        input: Payload,
    ) -> Result<Vec<(String, Payload)>, KernelError> {
        let subscribers = self.subscribers(hook);
        match hook.mode() {
            HookMode::Pipeline => {
                let mut current = input;
                let mut last = None;
                for entry in subscribers {
                    let ctx = Context::new(&entry.manifest.plugin.name, &entry.capabilities);
                    current = entry.plugin.on_hook(&ctx, hook, current)?;
                    last = Some(entry.manifest.plugin.name.clone());
                }
                Ok(last.map(|name| (name, current)).into_iter().collect())
            }
            HookMode::FanOut => subscribers
                .into_iter()
                .map(|entry| {
                    let ctx = Context::new(&entry.manifest.plugin.name, &entry.capabilities);
                    let out = entry.plugin.on_hook(&ctx, hook, input.clone())?;
                    Ok((entry.manifest.plugin.name.clone(), out))
                })
                .collect(),
        }
    }
}
