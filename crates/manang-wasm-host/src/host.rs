//! Host-side implementations of the interfaces a plugin may import.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use manang_kernel::{Access, BufferService, Capability, CapabilitySet};
use parking_lot::RwLock;
use wasmtime::StoreLimits;

use crate::boundary::truncate_message;
use crate::fs_host::FsHost;
use crate::http_host;
use crate::limits::Limits;
use crate::runtime::bindings::manang::plugin::{buffer, fs, http, types::Failure};

/// The directory `fs` operations are rooted at. Shared by every plugin so the
/// shell can switch projects without reloading plugins; `None` means no
/// project is open and `fs` calls fail.
#[derive(Debug, Default)]
pub struct ProjectRoot(RwLock<Option<PathBuf>>);

impl ProjectRoot {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self(RwLock::new(path))
    }

    pub fn set(&self, path: Option<PathBuf>) {
        *self.0.write() = path;
    }

    pub fn get(&self) -> Option<PathBuf> {
        self.0.read().clone()
    }
}

/// Host services handed to plugins, each reachable only through the matching
/// capability.
#[derive(Clone, Default)]
pub struct HostServices {
    pub buffer: Option<Arc<dyn BufferService>>,
    pub project: Arc<ProjectRoot>,
}

/// Per-call store state.
pub(crate) struct HostState {
    pub(crate) plugin: String,
    pub(crate) capabilities: CapabilitySet,
    pub(crate) services: HostServices,
    pub(crate) limits: StoreLimits,
    pub(crate) call_limits: Limits,
    /// When the call began; the epoch callback measures against this.
    pub(crate) started: Instant,
    /// Wall-clock time spent in host I/O, which the guest deadline excludes.
    pub(crate) io_used: Duration,
    /// Bytes of `fs` traffic this call may still move.
    pub(crate) fs_budget: usize,
}

fn fail(message: impl Into<String>) -> Failure {
    Failure {
        message: truncate_message(message.into()),
    }
}

impl HostState {
    fn require(&self, wanted: Capability) -> Result<(), Failure> {
        if self.capabilities.allows(&wanted) {
            Ok(())
        } else {
            Err(fail(format!(
                "plugin `{}` lacks the capability {wanted}",
                self.plugin
            )))
        }
    }

    fn buffer(&self) -> Result<&Arc<dyn BufferService>, Failure> {
        self.services
            .buffer
            .as_ref()
            .ok_or_else(|| fail("no buffer is available"))
    }

    /// Run `f` as host I/O: refused once the call's I/O allowance is spent,
    /// and its duration is credited back against the guest deadline.
    fn timed_io<R>(
        &mut self,
        f: impl FnOnce(&Limits, &CapabilitySet, Duration) -> Result<R, String>,
    ) -> Result<R, Failure> {
        let allowance = self.call_limits.io_time.saturating_sub(self.io_used);
        if allowance.is_zero() {
            return Err(fail("this call has used its host I/O time allowance"));
        }
        let timeout = allowance.min(self.call_limits.http_timeout);
        let started = Instant::now();
        let out = f(&self.call_limits, &self.capabilities, timeout);
        self.io_used += started.elapsed();
        out.map_err(fail)
    }

    fn with_fs<R>(
        &mut self,
        f: impl FnOnce(&FsHost<'_>, &mut usize) -> Result<R, String>,
    ) -> Result<R, Failure> {
        let root = self
            .services
            .project
            .get()
            .ok_or_else(|| fail("no project is open"))?;
        let mut budget = self.fs_budget;
        let out = self.timed_io(|limits, capabilities, _| {
            let host = FsHost {
                root: &root,
                capabilities,
                limits,
            };
            f(&host, &mut budget)
        });
        self.fs_budget = budget;
        out
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
        self.buffer()?.read_active().map_err(fail)
    }

    fn write_active(&mut self, text: String) -> Result<(), Failure> {
        self.require(active(Access::Write))?;
        self.buffer()?.write_active(&text).map_err(fail)
    }
}

impl fs::Host for HostState {
    fn read_file(&mut self, path: String) -> Result<Vec<u8>, Failure> {
        self.with_fs(|host, budget| host.read_file(&path, budget))
    }

    fn write_file(&mut self, path: String, data: Vec<u8>) -> Result<(), Failure> {
        self.with_fs(|host, budget| host.write_file(&path, &data, budget))
    }

    fn list_dir(&mut self, path: String) -> Result<Vec<fs::DirEntry>, Failure> {
        self.with_fs(|host, _| host.list_dir(&path)).map(|entries| {
            entries
                .into_iter()
                .map(|e| fs::DirEntry {
                    name: e.name,
                    is_dir: e.is_dir,
                    size: e.size,
                })
                .collect()
        })
    }
}

impl http::Host for HostState {
    fn fetch(&mut self, request: http::Request) -> Result<http::Response, Failure> {
        let request = http_host::Request {
            method: request.method,
            url: request.url,
            headers: request
                .headers
                .into_iter()
                .map(|h| (h.name, h.value))
                .collect(),
            body: request.body,
        };
        let response = self.timed_io(|limits, capabilities, timeout| {
            http_host::fetch(capabilities, &request, limits, timeout)
        })?;
        Ok(http::Response {
            status: response.status,
            headers: response
                .headers
                .into_iter()
                .map(|(name, value)| http::Header { name, value })
                .collect(),
            body: response.body,
        })
    }
}
