//! Sandboxed WASM tier of the Manang plugin system.
//!
//! Plugins are WebAssembly *components* speaking the `manang:plugin` WIT
//! world (`wit/plugin.wit`). The host links only the interfaces a plugin's
//! manifest grants, so a missing permission is a missing import, not a runtime
//! check a plugin could route around. There are no WASI imports: a plugin has
//! no ambient syscalls.
//!
//! Each call runs in a fresh [`wasmtime::Store`] with a memory cap and an epoch
//! deadline, so a plugin can neither retain state between calls nor hang the
//! host. Calls are CPU-bound and synchronous: run them on the Rayon pool, never
//! on a Tokio worker.

mod error;
mod host;
mod limits;
mod runtime;

pub use error::WasmError;
pub use limits::Limits;
pub use manang_kernel::BufferService;
pub use runtime::{WasmPlugin, WasmRuntime};
