//! The registry and dispatcher that hosts plugins.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::Serialize;

use crate::capability::CapabilitySet;
use crate::error::KernelError;
use crate::error::PluginError;
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

    /// Atomically swap the plugin registered under `manifest.plugin.name`.
    ///
    /// Used for hot reload. Calls already running keep their own `Arc` to the
    /// old plugin and finish on it; calls that start after this returns see the
    /// new one. The plugin keeps its place in hook pipeline order. On error the
    /// old plugin stays registered untouched.
    pub fn replace(&self, manifest: Manifest, plugin: Arc<dyn Plugin>) -> Result<(), KernelError> {
        manifest.validate()?;
        let capabilities = manifest.capabilities()?;
        let name = manifest.plugin.name.clone();

        let mut reg = self.registry.write();
        let old = reg
            .plugins
            .get(&name)
            .cloned()
            .ok_or_else(|| KernelError::UnknownPlugin(name.clone()))?;
        for command in &manifest.contributes.commands {
            if let Some(owner) = reg.commands.get(&command.id)
                && owner != &name
            {
                return Err(KernelError::CommandTaken {
                    command: command.id.clone(),
                    owner: owner.clone(),
                });
            }
        }
        for command in &old.manifest.contributes.commands {
            reg.commands.remove(&command.id);
        }
        for command in &manifest.contributes.commands {
            reg.commands.insert(command.id.clone(), name.clone());
        }
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

    /// Names of every registered plugin, in registration order.
    pub fn plugin_names(&self) -> Vec<String> {
        self.registry.read().order.clone()
    }

    /// A copy of a registered plugin's manifest.
    pub fn manifest(&self, name: &str) -> Option<Manifest> {
        self.registry
            .read()
            .plugins
            .get(name)
            .map(|e| e.manifest.clone())
    }

    /// Remove a plugin and everything it contributed.
    ///
    /// Calls already running finish on the removed plugin.
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
        self.invoke_at(command, input, 0)
    }

    /// Whether some registered plugin provides `command`.
    pub fn has_command(&self, command: &str) -> bool {
        self.registry.read().commands.contains_key(command)
    }

    /// [`Kernel::invoke`] at a nesting `depth`: 0 for the shell's own calls,
    /// one more for each [`Context::invoke`] on the way.
    pub(crate) fn invoke_at(
        &self,
        command: &str,
        input: Payload,
        depth: usize,
    ) -> Result<Payload, KernelError> {
        if depth > crate::plugin::MAX_CALL_DEPTH {
            return Err(KernelError::CallTooDeep(command.to_owned()));
        }
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
        let ctx = Context::new(
            &entry.manifest.plugin.name,
            &entry.capabilities,
            self,
            depth,
        );
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
    /// The result is a list of `(plugin, payload)`: a
    /// [`HookMode::Pipeline`] yields at most one entry (the last output), a
    /// [`HookMode::FanOut`] one entry per subscriber. The first failing
    /// subscriber aborts either. Use [`Kernel::run_hook_each`] when one broken
    /// plugin must not hide the others' results.
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
                    let ctx =
                        Context::new(&entry.manifest.plugin.name, &entry.capabilities, self, 0);
                    current = entry.plugin.on_hook(&ctx, hook, current)?;
                    last = Some(entry.manifest.plugin.name.clone());
                }
                Ok(last.map(|name| (name, current)).into_iter().collect())
            }
            HookMode::FanOut => subscribers
                .into_iter()
                .map(|entry| {
                    let ctx =
                        Context::new(&entry.manifest.plugin.name, &entry.capabilities, self, 0);
                    let out = entry.plugin.on_hook(&ctx, hook, input.clone())?;
                    Ok((entry.manifest.plugin.name.clone(), out))
                })
                .collect(),
        }
    }

    /// Like [`Kernel::run_hook`] but never aborts early: every subscriber
    /// reports its own result. A pipeline passes a failing subscriber's input
    /// through unchanged so later stages still run.
    pub fn run_hook_each(
        &self,
        hook: Hook,
        input: Payload,
    ) -> Vec<(String, Result<Payload, PluginError>)> {
        let subscribers = self.subscribers(hook);
        let mut current = input.clone();
        subscribers
            .into_iter()
            .map(|entry| {
                let ctx = Context::new(&entry.manifest.plugin.name, &entry.capabilities, self, 0);
                let arg = match hook.mode() {
                    HookMode::Pipeline => current.clone(),
                    HookMode::FanOut => input.clone(),
                };
                let out = entry.plugin.on_hook(&ctx, hook, arg);
                if let (HookMode::Pipeline, Ok(next)) = (hook.mode(), &out) {
                    current = next.clone();
                }
                (entry.manifest.plugin.name.clone(), out)
            })
            .collect()
    }
}
