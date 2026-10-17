//! Tectonic, driven in-process by the worker binary. Feature `engine` only.
//!
//! Tectonic's C engines keep process-global state and cannot be interrupted,
//! so the plugin never calls [`run`] on its own threads: it runs it in a
//! short-lived `manang-latex-worker` process that it can kill on timeout.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use tectonic::config::PersistentConfig;
use tectonic::driver::{OutputFormat, ProcessingSessionBuilder};
use tectonic::status::NoopStatusBackend;
use tectonic_bundles::detect_bundle;

use crate::engine::{FailureKind, WorkerOutcome, WorkerRequest};

/// Compile in this process. Blocks; CPU-bound.
pub fn run(request: &WorkerRequest) -> WorkerOutcome {
    let stem = request
        .main
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".to_owned());

    let bundle_url = request
        .bundle
        .clone()
        .unwrap_or_else(|| PersistentConfig::default().default_bundle_loc().to_owned());
    let bundle_cache = request.cache_dir.join("bundles");
    let format_cache = request.cache_dir.join("formats");
    for dir in [&bundle_cache, &format_cache] {
        if let Err(e) = fs::create_dir_all(dir) {
            return WorkerOutcome::failed(
                FailureKind::Engine,
                format!("{}: {e}", dir.display()),
                stem,
            );
        }
    }

    let bundle = match detect_bundle(bundle_url.clone(), request.only_cached, Some(bundle_cache)) {
        Ok(Some(bundle)) => bundle,
        Ok(None) => {
            return WorkerOutcome::failed(
                FailureKind::Bundle,
                format!("`{bundle_url}` is not a recognised bundle location"),
                stem,
            );
        }
        Err(e) => {
            return WorkerOutcome::failed(FailureKind::Bundle, format!("{e:#}"), stem);
        }
    };

    let mut status = NoopStatusBackend::default();
    let mut builder = ProcessingSessionBuilder::default();
    builder
        .bundle(bundle)
        .primary_input_path(&request.main)
        .tex_input_name(&format!("{stem}.tex"))
        .filesystem_root(&request.root)
        .output_dir(&request.out_dir)
        .format_name("latex")
        .format_cache_path(&format_cache)
        .output_format(OutputFormat::Pdf)
        .keep_logs(true)
        .keep_intermediates(false)
        .print_stdout(false)
        .synctex(request.synctex)
        .build_date(SystemTime::UNIX_EPOCH + Duration::from_secs(request.build_epoch))
        .shell_escape_disabled();

    let mut session = match builder.create(&mut status) {
        Ok(session) => session,
        Err(e) => return WorkerOutcome::failed(FailureKind::Bundle, format!("{e:#}"), stem),
    };
    match session.run(&mut status) {
        Ok(()) => WorkerOutcome {
            failure: None,
            message: String::new(),
            stem,
        },
        Err(e) => {
            let message = format!("{e:#}");
            let kind = if request.only_cached
                && (message.contains("cache") || message.contains("tectonic-format"))
            {
                FailureKind::Bundle
            } else {
                FailureKind::Tex
            };
            WorkerOutcome::failed(kind, message, stem)
        }
    }
}

/// Read a request, run it, and write `outcome.json` into its output directory.
pub fn run_from_file(request_path: &Path) -> Result<(), String> {
    let bytes = fs::read(request_path).map_err(|e| format!("{}: {e}", request_path.display()))?;
    let request: WorkerRequest =
        serde_json::from_slice(&bytes).map_err(|e| format!("bad request: {e}"))?;
    let outcome = run(&request);
    let json = serde_json::to_vec(&outcome).map_err(|e| e.to_string())?;
    let path = request.out_dir.join("outcome.json");
    fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))
}
