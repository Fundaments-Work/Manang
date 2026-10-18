//! Test plugins built from WAT, so the manager tests need no guest toolchain.
// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// A component of world `plugin` whose `invoke` spins `spin` loop iterations
/// and then returns `ok(json(<answer>))`, where `answer` is one ASCII digit.
pub fn component(answer: char, spin: u64) -> Vec<u8> {
    let wat = format!(
        r#"(module
  (memory (export "memory") 1)
  (data (i32.const 1024) "\00\00\00\00\01\00\00\00\10\04\00\00\01\00\00\00")
  (data (i32.const 1040) "{answer}")
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32) i32.const 2048)
  (func (export "manang:plugin/handler@0.2.0#invoke")
    (param i32 i32 i32 i32 i32) (result i32)
    (local $i i64)
    (loop $l
      (local.set $i (i64.add (local.get $i) (i64.const 1)))
      (br_if $l (i64.lt_u (local.get $i) (i64.const {spin}))))
    i32.const 1024)
  (func (export "manang:plugin/handler@0.2.0#on-hook")
    (param i32 i32 i32 i32) (result i32)
    i32.const 1024)
)"#
    );
    let mut module = wat::parse_str(wat).expect("valid WAT");
    let wit = Path::new(env!("CARGO_MANIFEST_DIR")).join("../manang-wasm-host/wit");
    let mut resolve = wit_parser::Resolve::default();
    let (package, _) = resolve.push_dir(wit).expect("host WIT parses");
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

pub fn manifest_toml(name: &str) -> String {
    format!(
        "[plugin]\nname = \"{name}\"\nversion = \"0.1.0\"\n\
         [[contributes.commands]]\nid = \"{name}.run\"\ntitle = \"Run\"\n"
    )
}

/// A manifest with a version and network permissions.
pub fn manifest_with(name: &str, version: &str, network: &[&str]) -> String {
    let hosts: Vec<String> = network.iter().map(|h| format!("\"{h}\"")).collect();
    format!(
        "[plugin]\nname = \"{name}\"\nversion = \"{version}\"\ndescription = \"Test plugin {name}\"\n\
         [permissions]\nnetwork = [{}]\n\
         [[contributes.commands]]\nid = \"{name}.run\"\ntitle = \"Run\"\n",
        hosts.join(", ")
    )
}

/// Write a plugin source folder `<parent>/<name>` and return its path.
pub fn folder(parent: &Path, name: &str, manifest: &str, wasm: &[u8]) -> PathBuf {
    let dir = parent.join(name);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("manifest.toml"), manifest).expect("manifest");
    std::fs::write(dir.join("plugin.wasm"), wasm).expect("wasm");
    dir
}

/// A `.manangplugin` archive at `path` holding `entries` (name, bytes).
pub fn archive(path: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
    use std::io::Write;
    let file = std::fs::File::create(path).expect("create archive");
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        zip.start_file(*name, options).expect("start file");
        zip.write_all(bytes).expect("write entry");
    }
    zip.finish().expect("finish");
    path.to_path_buf()
}

/// Install a plugin directory under `plugins`.
pub fn install(plugins: &Path, name: &str, wasm: &[u8]) -> PathBuf {
    let dir = plugins.join(name);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("manifest.toml"), manifest_toml(name)).expect("manifest");
    std::fs::write(dir.join("plugin.wasm"), wasm).expect("wasm");
    dir
}
