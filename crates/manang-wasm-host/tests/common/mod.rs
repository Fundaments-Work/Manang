//! Shared helpers for the integration tests: building guest components.
//!
//! Needs the `wasm32-unknown-unknown` target (`rustup target add
//! wasm32-unknown-unknown`).
// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use manang_kernel::Manifest;

/// Where the host's WIT lives.
pub fn wit_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("wit")
}

/// Compile a fixture crate to a component, once per test process.
fn build_fixture(name: &str, module_file: &str) -> Vec<u8> {
    static BUILD_LOCK: Mutex<()> = Mutex::new(());
    // Fixtures share one target directory so dependencies compile once.
    let _guard = BUILD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("guests");
    let status = Command::new(env!("CARGO"))
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .arg("--manifest-path")
        .arg(fixture.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        .status()
        .expect("cargo runs");
    assert!(status.success(), "building the {name} guest failed");
    let module = std::fs::read(
        target_dir
            .join("wasm32-unknown-unknown/release")
            .join(module_file),
    )
    .expect("guest module exists");
    wit_component::ComponentEncoder::default()
        .validate(true)
        .module(&module)
        .expect("module carries a component type")
        .encode()
        .expect("encodes as a component")
}

pub fn echo_guest() -> &'static [u8] {
    static C: OnceLock<Vec<u8>> = OnceLock::new();
    C.get_or_init(|| build_fixture("echo-guest", "echo_guest.wasm"))
}

pub fn io_guest() -> &'static [u8] {
    static C: OnceLock<Vec<u8>> = OnceLock::new();
    C.get_or_init(|| build_fixture("io-guest", "io_guest.wasm"))
}

/// Wrap a hand-written core module (WAT) as a component of world `plugin`
/// (handler export only). The module must export `memory`, `cabi_realloc` and
/// the two `manang:plugin/handler@0.2.0#...` functions.
pub fn wat_component(core_wat: &str) -> Vec<u8> {
    let mut module = wat::parse_str(core_wat).expect("valid WAT");
    let mut resolve = wit_parser::Resolve::default();
    let (package, _) = resolve.push_dir(wit_dir()).expect("host WIT parses");
    let world = resolve
        .select_world(&[package], Some("plugin"))
        .expect("world exists");
    wit_component::embed_component_metadata(
        &mut module,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
        false,
    )
    .expect("metadata embeds");
    wit_component::ComponentEncoder::default()
        .validate(true)
        .module(&module)
        .expect("module carries a component type")
        .encode()
        .expect("encodes as a component")
}

/// A core module whose `invoke` and `on-hook` both return `ok(empty)`.
/// `extra` is spliced into the module body for scenario-specific items;
/// `invoke_body` runs before returning.
pub fn wat_plugin(extra: &str, invoke_body: &str) -> String {
    format!(
        r#"(module
  (memory (export "memory") 1)
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32) i32.const 2048)
  (func (export "manang:plugin/handler@0.2.0#invoke")
    (param i32 i32 i32 i32 i32) (result i32)
    {invoke_body}
    i32.const 1024)
  (func (export "manang:plugin/handler@0.2.0#on-hook")
    (param i32 i32 i32 i32) (result i32)
    i32.const 1024)
  {extra}
)"#
    )
}

pub fn manifest(src: &str) -> Manifest {
    Manifest::from_toml(src).expect("valid manifest")
}
