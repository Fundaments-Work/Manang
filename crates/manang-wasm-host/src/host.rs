//! Host-side implementations of the interfaces a plugin may import.

use std::sync::Arc;

use manang_kernel::{Access, BufferService, Capability, CapabilitySet};
use wasmtime::StoreLimits;

use crate::runtime::bindings::manang::plugin::{buffer, types::Failure};

/// Per-call store state.
pub(crate) struct HostState {
    pub(crate) plugin: String,
    pub(crate) capabilities: CapabilitySet,
    pub(crate) buffer: Option<Arc<dyn BufferService>>,
    pub(crate) limits: StoreLimits,
}

impl HostState {
    fn require(&self, wanted: Capability) -> Result<(), Failure> {
        if self.capabilities.allows(&wanted) {
            Ok(())
        } else {
            Err(Failure {
                message: format!("plugin `{}` lacks the capability {wanted}", self.plugin),
            })
        }
    }

    fn buffer(&self) -> Result<&Arc<dyn BufferService>, Failure> {
        self.buffer.as_ref().ok_or_else(|| Failure {
            message: "no buffer is available".to_owned(),
        })
    }
}

fn active(access: Access) -> Capability {
    Capability::Buffer {
        access,
        target: "active".to_owned(),
    }
}

impl buffer::Host for HostState {
    fn read_active(&mut self) -> Result<String, Failure> {
        self.require(active(Access::Read))?;
        self.buffer()?
            .read_active()
            .map_err(|message| Failure { message })
    }

    fn write_active(&mut self, text: String) -> Result<(), Failure> {
        self.require(active(Access::Write))?;
        self.buffer()?
            .write_active(&text)
            .map_err(|message| Failure { message })
    }
}
