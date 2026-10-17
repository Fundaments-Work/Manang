//! Sandboxed WASM tier of the Manang plugin system.
//!
//! Plugins are WebAssembly *components* speaking the `manang:plugin` WIT
//! world (`wit/plugin.wit`). The host links only the interfaces a plugin's
//! manifest grants, so a missing permission is a missing import, not a runtime
//! check a plugin could route around. There are no WASI imports: a plugin has
//! no ambient syscalls.
//!
//! Each call runs in a fresh [`wasmtime::Store`] with memory, table and
//! instance caps and an epoch deadline that covers instantiation too, so a
//! plugin can neither retain state between calls nor hang the host. Payloads
//! are size-capped in both directions ([`boundary`]). Calls are CPU-bound and
//! synchronous: run them on the Rayon pool, never on a Tokio worker.
//!
//! Cold starts are served from a tamper-evident precompiled cache
//! ([`ComponentCache`]); it holds the crate's only `unsafe` code.

#![deny(unsafe_code)]

pub mod boundary;
mod cache;
mod error;
mod fs_host;
mod host;
mod http_host;
mod limits;
mod runtime;

pub use cache::{CacheStatus, ComponentCache};
pub use error::{CacheError, WasmError};
pub use host::{HostServices, ProjectRoot};
pub use limits::Limits;
pub use manang_kernel::BufferService;
pub use runtime::{RuntimeOptions, WIT_VERSION, WasmPlugin, WasmRuntime};
