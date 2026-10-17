use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use manang_kernel::{CapabilitySet, Context, Hook, Manifest, Payload, Plugin, PluginError, Tier};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, OptLevel, Store, StoreLimitsBuilder};

use crate::error::WasmError;
use crate::host::{BufferService, HostState};
use crate::limits::Limits;

pub(crate) mod bindings {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "bindings",
    });
}

use bindings::manang::plugin::types as wit;

/// How often the epoch advances. Deadlines are rounded up to this.
const EPOCH_TICK: Duration = Duration::from_millis(10);

/// Interface names a component may import, and nothing else.
const BUFFER_IMPORT: &str = "manang:plugin/buffer@0.1.0";
/// Pure type definitions: no functions, so nothing to grant or sandbox.
const TYPES_IMPORT: &str = "manang:plugin/types@0.1.0";

/// Advances the engine epoch so deadlines fire. Stops when dropped.
struct EpochTicker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl EpochTicker {
    fn start(engine: Engine) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("manang-wasm-epoch".to_owned())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    thread::sleep(EPOCH_TICK);
                    engine.increment_epoch();
                }
            })
            .ok();
        Self { stop, handle }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            // A panicked ticker has nothing further to clean up.
            let _ = handle.join();
        }
    }
}

/// Shared WASM engine. Create one per process and load every plugin from it.
pub struct WasmRuntime {
    engine: Engine,
    limits: Limits,
    _ticker: EpochTicker,
}

impl WasmRuntime {
    pub fn new(limits: Limits) -> Result<Self, WasmError> {
        let mut config = Config::new();
        config
            .wasm_component_model(true)
            .epoch_interruption(true)
            .cranelift_opt_level(OptLevel::Speed);
        let engine = Engine::new(&config).map_err(|e| WasmError::Engine(e.to_string()))?;
        let ticker = EpochTicker::start(engine.clone());
        Ok(Self {
            engine,
            limits,
            _ticker: ticker,
        })
    }

    /// Compile and link a component against exactly the interfaces its
    /// manifest grants.
    pub fn load(
        &self,
        manifest: &Manifest,
        wasm: &[u8],
        buffer: Option<Arc<dyn BufferService>>,
    ) -> Result<WasmPlugin, WasmError> {
        let name = manifest.plugin.name.clone();
        if manifest.plugin.tier != Tier::Wasm {
            return Err(WasmError::WrongTier(name));
        }
        let capabilities = manifest.capabilities()?;
        let invalid = |reason: String| WasmError::InvalidComponent {
            plugin: name.clone(),
            reason,
        };

        let component = Component::new(&self.engine, wasm).map_err(|e| invalid(e.to_string()))?;

        let grants_buffer = !manifest.permissions.buffer.is_empty();
        let mut linker: Linker<HostState> = Linker::new(&self.engine);
        for (import, _) in component.component_type().imports(&self.engine) {
            if import == BUFFER_IMPORT {
                if !grants_buffer {
                    return Err(WasmError::UngrantedImport {
                        plugin: name,
                        interface: import.to_owned(),
                    });
                }
                bindings::manang::plugin::buffer::add_to_linker::<_, HasSelf<_>>(
                    &mut linker,
                    |state| state,
                )
                .map_err(|e| WasmError::Engine(e.to_string()))?;
            } else if import == TYPES_IMPORT {
                linker
                    .instance(TYPES_IMPORT)
                    .map_err(|e| WasmError::Engine(e.to_string()))?;
            } else {
                return Err(WasmError::UnknownImport {
                    plugin: name,
                    interface: import.to_owned(),
                });
            }
        }

        let pre = linker
            .instantiate_pre(&component)
            .map_err(|e| WasmError::Instantiate {
                plugin: name.clone(),
                reason: e.to_string(),
            })?;
        let pre = bindings::BindingsPre::new(pre).map_err(|e| WasmError::Instantiate {
            plugin: name.clone(),
            reason: e.to_string(),
        })?;

        Ok(WasmPlugin {
            name,
            engine: self.engine.clone(),
            pre,
            capabilities,
            buffer,
            limits: self.limits,
        })
    }
}

/// A loaded, sandboxed plugin. Implements the kernel's [`Plugin`] trait.
pub struct WasmPlugin {
    name: String,
    engine: Engine,
    pre: bindings::BindingsPre<HostState>,
    capabilities: CapabilitySet,
    buffer: Option<Arc<dyn BufferService>>,
    limits: Limits,
}

fn to_wit(payload: &Payload) -> wit::Payload {
    match payload {
        Payload::Empty => wit::Payload::Empty,
        Payload::Json(value) => wit::Payload::Json(value.to_string()),
        Payload::Binary(bytes) => wit::Payload::Binary(bytes.to_vec()),
    }
}

fn from_wit(payload: wit::Payload) -> Result<Payload, PluginError> {
    Ok(match payload {
        wit::Payload::Empty => Payload::Empty,
        wit::Payload::Json(text) => Payload::Json(
            serde_json::from_str(&text)
                .map_err(|e| PluginError::Failed(format!("plugin returned invalid JSON: {e}")))?,
        ),
        wit::Payload::Binary(bytes) => Payload::Binary(bytes.into()),
    })
}

impl WasmPlugin {
    fn store(&self) -> Store<HostState> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.limits.memory_bytes)
            .instances(16)
            .memories(4)
            .tables(4)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(
            &self.engine,
            HostState {
                plugin: self.name.clone(),
                capabilities: self.capabilities.clone(),
                buffer: self.buffer.clone(),
                limits,
            },
        );
        store.limiter(|state| &mut state.limits);
        let ticks = self
            .limits
            .deadline
            .as_millis()
            .div_ceil(EPOCH_TICK.as_millis()) as u64;
        store.set_epoch_deadline(ticks.max(1));
        store
    }

    fn call<R>(
        &self,
        f: impl FnOnce(
            &bindings::exports::manang::plugin::handler::Guest,
            &mut Store<HostState>,
        ) -> wasmtime::Result<Result<R, wit::Failure>>,
    ) -> Result<R, PluginError> {
        let mut store = self.store();
        let instance = self
            .pre
            .instantiate(&mut store)
            .map_err(|e| PluginError::Failed(format!("instantiation failed: {e}")))?;
        match f(instance.manang_plugin_handler(), &mut store) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(failure)) => Err(PluginError::Failed(failure.message)),
            Err(trap) => Err(PluginError::Failed(format!("plugin trapped: {trap}"))),
        }
    }
}

impl Plugin for WasmPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let input = to_wit(&input);
        let out = self.call(|guest, store| guest.call_invoke(store, command, &input))?;
        from_wit(out)
    }

    fn on_hook(
        &self,
        _ctx: &Context<'_>,
        hook: Hook,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let hook = match hook {
            Hook::OnCompilePre => wit::Hook::OnCompilePre,
            Hook::OnCompilePost => wit::Hook::OnCompilePost,
            Hook::OnLint => wit::Hook::OnLint,
            Hook::RegisterAgentTool => wit::Hook::RegisterAgentTool,
        };
        let input = to_wit(&input);
        let out = self.call(|guest, store| guest.call_on_hook(store, hook, &input))?;
        from_wit(out)
    }
}
