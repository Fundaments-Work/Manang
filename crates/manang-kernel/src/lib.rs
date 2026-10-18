//! Manang plugin kernel.
//!
//! Every Manang feature is a plugin. A plugin declares itself in a
//! `manifest.toml` (identity, capabilities, contributed commands and hooks) and
//! is hosted in one of two tiers behind the same [`Plugin`] API:
//!
//! * [`Tier::Native`]: first-party, trusted Rust compiled into the binary, so
//!   hot paths stay inside the latency budget.
//! * [`Tier::Wasm`]: third-party, run in a Wasmtime sandbox with no
//!   syscall access beyond the capabilities the manifest declares.
//!
//! The kernel owns no UI and no Tauri types. The shell talks to it through
//! command ids and [`Payload`]s only.

pub mod agent;
pub mod capability;
pub mod error;
pub mod events;
pub mod kernel;
pub mod manifest;
pub mod payload;
pub mod plugin;
pub mod services;

pub use agent::{AgentTool, AgentToolError, AgentToolList, RegisteredTool, SchemaIssue};
pub use capability::{Access, Capability, CapabilitySet};
pub use error::{CapabilityError, KernelError, ManifestError, PluginError};
pub use events::{Event, EventBus, Subscription};
pub use kernel::{CommandInfo, Kernel};
pub use manifest::{
    CommandSpec, Contributes, Hook, HookMode, Manifest, MenuBar, MenuPlacement, MenuSpec,
    PluginMeta, SettingKind, SettingScope, SettingSpec, Tier,
};
pub use payload::Payload;
pub use plugin::{Context, MAX_CALL_DEPTH, Plugin};
pub use services::BufferService;
