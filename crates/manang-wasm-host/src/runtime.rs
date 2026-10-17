use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use manang_kernel::{
    BufferService, CapabilitySet, Context, Hook, Manifest, Payload, Plugin, PluginError, Tier,
};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{
    Config, Engine, InstanceAllocationStrategy, OptLevel, PoolingAllocationConfig, Store,
    StoreLimitsBuilder, UpdateDeadline,
};

use crate::boundary::{RawPayload, decode_output, encode_input, truncate_message};
use crate::cache::{CacheStatus, ComponentCache};
use crate::error::{CacheError, WasmError};
use crate::host::{HostServices, HostState};
use crate::limits::{Limits, MAX_INSTANCES, MAX_MEMORIES, MAX_TABLES};

pub(crate) mod bindings {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "bindings",
    });
}

use bindings::manang::plugin::types as wit;

/// The WIT package version this host implements.
pub const WIT_VERSION: &str = "0.2.0";

/// How often the epoch advances. Deadlines are rounded up to this.
const EPOCH_TICK: Duration = Duration::from_millis(10);

/// Interface names a component may import, and nothing else.
const BUFFER_IMPORT: &str = "manang:plugin/buffer@0.2.0";
const FS_IMPORT: &str = "manang:plugin/fs@0.2.0";
const HTTP_IMPORT: &str = "manang:plugin/http@0.2.0";
/// Pure type definitions: no functions, so nothing to grant or sandbox.
const TYPES_IMPORT: &str = "manang:plugin/types@0.2.0";

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

/// How the shared engine is built.
#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub limits: Limits,
    /// Use Wasmtime's pooling instance allocator. It pre-reserves address
    /// space for `pool_slots` concurrent instances so a call never pays for
    /// `mmap`/`munmap`.
    pub pooling: bool,
    /// Concurrent plugin instances the pool supports. Calls beyond it fail
    /// with an instantiation error rather than blocking.
    pub pool_slots: u32,
    /// Directory for the precompiled-component cache. `None` disables it.
    pub cache_dir: Option<PathBuf>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            pooling: true,
            pool_slots: 64,
            cache_dir: None,
        }
    }
}

/// Shared WASM engine. Create one per process and load every plugin from it.
pub struct WasmRuntime {
    engine: Engine,
    limits: Limits,
    cache: Option<ComponentCache>,
    pooling: bool,
    _ticker: EpochTicker,
}

fn engine_config(options: &RuntimeOptions, pooling: bool) -> Config {
    let mut config = Config::new();
    config
        .wasm_component_model(true)
        .epoch_interruption(true)
        .cranelift_opt_level(OptLevel::Speed);
    if pooling {
        let slots = options.pool_slots.max(1);
        let mut pool = PoolingAllocationConfig::new();
        pool.total_component_instances(slots)
            .total_core_instances(slots.saturating_mul(MAX_INSTANCES as u32 / 2))
            .total_memories(slots.saturating_mul(MAX_MEMORIES as u32 / 2))
            .total_tables(slots.saturating_mul(MAX_TABLES as u32 / 2))
            .max_core_instances_per_component(MAX_INSTANCES as u32)
            .max_memories_per_component(MAX_MEMORIES as u32)
            .max_tables_per_component(MAX_TABLES as u32)
            .max_memories_per_module(MAX_MEMORIES as u32)
            .max_tables_per_module(MAX_TABLES as u32)
            .max_memory_size(options.limits.memory_bytes)
            .table_elements(options.limits.table_elements)
            .max_core_instance_size(4 * 1024 * 1024)
            .max_component_instance_size(2 * 1024 * 1024);
        config.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));
    }
    config
}

impl WasmRuntime {
    /// An on-demand engine with default options and no cache.
    pub fn new(limits: Limits) -> Result<Self, WasmError> {
        Self::with_options(RuntimeOptions {
            limits,
            pooling: false,
            ..RuntimeOptions::default()
        })
    }

    /// Build an engine from explicit options. If the pooling allocator cannot
    /// reserve its address space the engine falls back to on-demand
    /// allocation rather than failing.
    pub fn with_options(options: RuntimeOptions) -> Result<Self, WasmError> {
        let (engine, pooling) = if options.pooling {
            match Engine::new(&engine_config(&options, true)) {
                Ok(engine) => (engine, true),
                Err(_) => (
                    Engine::new(&engine_config(&options, false))
                        .map_err(|e| WasmError::Engine(e.to_string()))?,
                    false,
                ),
            }
        } else {
            (
                Engine::new(&engine_config(&options, false))
                    .map_err(|e| WasmError::Engine(e.to_string()))?,
                false,
            )
        };
        let cache = options
            .cache_dir
            .map(ComponentCache::open)
            .transpose()
            .map_err(|e: CacheError| WasmError::Engine(e.to_string()))?;
        let ticker = EpochTicker::start(engine.clone());
        Ok(Self {
            engine,
            limits: options.limits,
            cache,
            pooling,
            _ticker: ticker,
        })
    }

    /// Whether the pooling allocator is active.
    pub fn pooling(&self) -> bool {
        self.pooling
    }

    /// Compile and link a component against exactly the interfaces its
    /// manifest grants. Only the `buffer` service is available; use
    /// [`WasmRuntime::load_with`] to provide a project directory.
    pub fn load(
        &self,
        manifest: &Manifest,
        wasm: &[u8],
        buffer: Option<Arc<dyn BufferService>>,
    ) -> Result<WasmPlugin, WasmError> {
        self.load_with(
            manifest,
            wasm,
            HostServices {
                buffer,
                ..HostServices::default()
            },
        )
    }

    /// Like [`WasmRuntime::load`] with the full set of host services.
    pub fn load_with(
        &self,
        manifest: &Manifest,
        wasm: &[u8],
        services: HostServices,
    ) -> Result<WasmPlugin, WasmError> {
        let name = manifest.plugin.name.clone();
        if manifest.plugin.tier != Tier::Wasm {
            return Err(WasmError::WrongTier(name));
        }
        let capabilities = manifest.capabilities()?;
        let invalid = |reason: String| WasmError::InvalidComponent {
            plugin: name.clone(),
            reason: truncate_message(reason),
        };

        let (component, cache_status) = match &self.cache {
            Some(cache) => cache
                .load_or_compile(&self.engine, wasm)
                .map_err(|e| invalid(e.to_string()))?,
            None => (
                Component::new(&self.engine, wasm).map_err(|e| invalid(e.to_string()))?,
                CacheStatus::Disabled,
            ),
        };

        let mut linker: Linker<HostState> = Linker::new(&self.engine);
        let engine_err = |e: wasmtime::Error| WasmError::Engine(e.to_string());
        for (import, _) in component.component_type().imports(&self.engine) {
            let ungranted = |granted: bool| {
                if granted {
                    Ok(())
                } else {
                    Err(WasmError::UngrantedImport {
                        plugin: name.clone(),
                        interface: import.to_owned(),
                    })
                }
            };
            match import {
                BUFFER_IMPORT => {
                    ungranted(capabilities.has_buffer())?;
                    bindings::manang::plugin::buffer::add_to_linker::<_, HasSelf<_>>(
                        &mut linker,
                        |state| state,
                    )
                    .map_err(engine_err)?;
                }
                FS_IMPORT => {
                    ungranted(capabilities.has_filesystem())?;
                    bindings::manang::plugin::fs::add_to_linker::<_, HasSelf<_>>(
                        &mut linker,
                        |state| state,
                    )
                    .map_err(engine_err)?;
                }
                HTTP_IMPORT => {
                    ungranted(capabilities.has_network())?;
                    bindings::manang::plugin::http::add_to_linker::<_, HasSelf<_>>(
                        &mut linker,
                        |state| state,
                    )
                    .map_err(engine_err)?;
                }
                TYPES_IMPORT => {
                    linker.instance(TYPES_IMPORT).map_err(engine_err)?;
                }
                other if other.starts_with("manang:plugin/") => {
                    return Err(WasmError::IncompatibleApi {
                        plugin: name,
                        found: other.to_owned(),
                        expected: WIT_VERSION,
                    });
                }
                other => {
                    return Err(WasmError::UnknownImport {
                        plugin: name,
                        interface: other.to_owned(),
                    });
                }
            }
        }

        let instantiate = |reason: String| WasmError::Instantiate {
            plugin: name.clone(),
            reason: truncate_message(reason),
        };
        let pre = linker
            .instantiate_pre(&component)
            .map_err(|e| instantiate(e.to_string()))?;
        let pre = bindings::BindingsPre::new(pre).map_err(|e| {
            // A guest built against another WIT version exports a differently
            // versioned handler; say so instead of a bare "not found".
            let text = e.to_string();
            if text.contains("manang:plugin/handler") {
                WasmError::IncompatibleApi {
                    plugin: name.clone(),
                    found: "a different `manang:plugin/handler` version".to_owned(),
                    expected: WIT_VERSION,
                }
            } else {
                instantiate(text)
            }
        })?;

        Ok(WasmPlugin {
            name,
            engine: self.engine.clone(),
            pre,
            capabilities,
            services,
            limits: self.limits,
            cache_status,
        })
    }
}

/// A loaded, sandboxed plugin. Implements the kernel's [`Plugin`] trait.
pub struct WasmPlugin {
    name: String,
    engine: Engine,
    pre: bindings::BindingsPre<HostState>,
    capabilities: CapabilitySet,
    services: HostServices,
    limits: Limits,
    cache_status: CacheStatus,
}

fn ticks(duration: Duration) -> u64 {
    (duration.as_millis().div_ceil(EPOCH_TICK.as_millis()) as u64).max(1)
}

impl From<RawPayload> for wit::Payload {
    fn from(raw: RawPayload) -> Self {
        match raw {
            RawPayload::Empty => Self::Empty,
            RawPayload::Json(text) => Self::Json(text),
            RawPayload::Binary(bytes) => Self::Binary(bytes),
        }
    }
}

impl From<wit::Payload> for RawPayload {
    fn from(payload: wit::Payload) -> Self {
        match payload {
            wit::Payload::Empty => Self::Empty,
            wit::Payload::Json(text) => Self::Json(text),
            wit::Payload::Binary(bytes) => Self::Binary(bytes),
        }
    }
}

fn panic_text(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic".to_owned())
}

impl WasmPlugin {
    /// The plugin's manifest name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the component came from the precompiled cache.
    pub fn cache_status(&self) -> &CacheStatus {
        &self.cache_status
    }

    fn store(&self) -> Store<HostState> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.limits.memory_bytes)
            .table_elements(self.limits.table_elements)
            .instances(MAX_INSTANCES)
            .memories(MAX_MEMORIES)
            .tables(MAX_TABLES)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(
            &self.engine,
            HostState {
                plugin: self.name.clone(),
                capabilities: self.capabilities.clone(),
                services: self.services.clone(),
                limits,
                call_limits: self.limits,
                started: Instant::now(),
                io_used: Duration::ZERO,
                fs_budget: self.limits.max_fs_call_bytes,
            },
        );
        store.limiter(|state| &mut state.limits);
        // The deadline is armed before instantiation, so start functions and
        // initialisers are bounded exactly like calls. When it fires, extend
        // it by the time spent in host I/O, otherwise interrupt.
        let guest_budget = self.limits.deadline;
        store.set_epoch_deadline(ticks(guest_budget));
        store.epoch_deadline_callback(move |ctx| {
            let state = ctx.data();
            let allowed = guest_budget + state.io_used;
            let elapsed = state.started.elapsed();
            match allowed.checked_sub(elapsed).filter(|d| !d.is_zero()) {
                Some(left) => Ok(UpdateDeadline::Continue(ticks(left))),
                None => Ok(UpdateDeadline::Interrupt),
            }
        });
        store
    }

    fn call_inner<R>(
        &self,
        f: impl FnOnce(
            &bindings::exports::manang::plugin::handler::Guest,
            &mut Store<HostState>,
        ) -> wasmtime::Result<Result<R, wit::Failure>>,
    ) -> Result<R, PluginError> {
        let mut store = self.store();
        let instance = self.pre.instantiate(&mut store).map_err(|e| {
            PluginError::Failed(truncate_message(format!("instantiation failed: {e}")))
        })?;
        match f(instance.manang_plugin_handler(), &mut store) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(failure)) => Err(PluginError::Failed(truncate_message(failure.message))),
            Err(trap) => Err(PluginError::Failed(truncate_message(format!(
                "plugin trapped: {trap}"
            )))),
        }
    }

    /// Run one guest call. A panic anywhere beneath it (a host service, a
    /// bug in this crate) is caught here, at the boundary, and reported as a
    /// plugin failure; the per-call store is dropped, so no state survives.
    ///
    /// Note: `catch_unwind` only works when the final binary is built with
    /// `panic = "unwind"`.
    fn call<R>(
        &self,
        f: impl FnOnce(
            &bindings::exports::manang::plugin::handler::Guest,
            &mut Store<HostState>,
        ) -> wasmtime::Result<Result<R, wit::Failure>>,
    ) -> Result<R, PluginError> {
        catch_unwind(AssertUnwindSafe(|| self.call_inner(f))).unwrap_or_else(|panic| {
            Err(PluginError::Failed(truncate_message(format!(
                "plugin host panicked: {}",
                panic_text(panic.as_ref())
            ))))
        })
    }
}

impl Plugin for WasmPlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let input: wit::Payload = encode_input(&input, &self.limits)?.into();
        let out = self.call(|guest, store| guest.call_invoke(store, command, &input))?;
        decode_output(out.into(), &self.limits)
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
        let input: wit::Payload = encode_input(&input, &self.limits)?.into();
        let out = self.call(|guest, store| guest.call_on_hook(store, hook, &input))?;
        decode_output(out.into(), &self.limits)
    }
}
