//! The single API every plugin implements, native or WASM-backed.

use crate::capability::{Capability, CapabilitySet};
use crate::error::PluginError;
use crate::manifest::Hook;
use crate::payload::Payload;

/// Capability-checked view of the host handed to a plugin on every call.
pub struct Context<'a> {
    plugin: &'a str,
    capabilities: &'a CapabilitySet,
}

impl<'a> Context<'a> {
    pub(crate) fn new(plugin: &'a str, capabilities: &'a CapabilitySet) -> Self {
        Self {
            plugin,
            capabilities,
        }
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
