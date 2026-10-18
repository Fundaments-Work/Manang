//! The single API every plugin implements, native or WASM-backed.

use crate::capability::{Capability, CapabilitySet};
use crate::error::{KernelError, PluginError};
use crate::kernel::Kernel;
use crate::manifest::Hook;
use crate::payload::Payload;

/// How deep plugins may call each other through [`Context::invoke`] before
/// the kernel refuses (a guard against accidental call cycles).
pub const MAX_CALL_DEPTH: usize = 8;

/// Capability-checked view of the host handed to a plugin on every call.
pub struct Context<'a> {
    plugin: &'a str,
    capabilities: &'a CapabilitySet,
    kernel: &'a Kernel,
    depth: usize,
}

impl<'a> Context<'a> {
    pub(crate) fn new(
        plugin: &'a str,
        capabilities: &'a CapabilitySet,
        kernel: &'a Kernel,
        depth: usize,
    ) -> Self {
        Self {
            plugin,
            capabilities,
            kernel,
            depth,
        }
    }

    /// Run another plugin's command, as the shell would.
    ///
    /// The callee runs with its own manifest's capabilities, never the
    /// caller's, so this grants nothing the shell could not already do. It is
    /// for first-party plugins composing each other (the project plugin
    /// writing what the diagram plugin renders); the WASM host does not
    /// expose it to sandboxed plugins. Nested calls deeper than
    /// [`MAX_CALL_DEPTH`] fail with [`KernelError::CallTooDeep`].
    pub fn invoke(&self, command: &str, input: Payload) -> Result<Payload, KernelError> {
        self.kernel.invoke_at(command, input, self.depth + 1)
    }

    /// Whether some registered plugin provides `command`.
    pub fn has_command(&self, command: &str) -> bool {
        self.kernel.has_command(command)
    }

    pub fn plugin(&self) -> &str {
        self.plugin
    }

    /// Fail unless the manifest granted `wanted`. Host services call this
    /// before touching anything on a plugin's behalf.
    pub fn require(&self, wanted: &Capability) -> Result<(), PluginError> {
        if self.capabilities.allows(wanted) {
            Ok(())
        } else {
            Err(PluginError::CapabilityDenied {
                plugin: self.plugin.to_owned(),
                capability: wanted.to_string(),
            })
        }
    }
}

/// A unit of Manang functionality.
///
/// Implementations must not hold kernel locks; the kernel releases its own
/// before calling in, so plugins may re-enter the kernel.
pub trait Plugin: Send + Sync {
    /// Handle one of the commands the manifest contributes.
    fn invoke(
        &self,
        ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError>;

    /// Handle a subscribed hook. The default passes the input through.
    fn on_hook(
        &self,
        _ctx: &Context<'_>,
        _hook: Hook,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        Ok(input)
    }
}
