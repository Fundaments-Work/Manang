//! The WASM tier against a real component built from `fixtures/echo-guest`.
//!
//! Needs the `wasm32-unknown-unknown` target (`rustup target add
//! wasm32-unknown-unknown`).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use manang_kernel::{Kernel, KernelError, Manifest, Payload};
use manang_wasm_host::{BufferService, Limits, WasmError, WasmRuntime};
use serde_json::json;

mod common;
use common::echo_guest as guest_component;

const BASE: &str = r#"
[plugin]
name = "echo"
version = "0.1.0"

[[contributes.commands]]
id = "echo.say"
title = "Say"
[[contributes.commands]]
id = "echo.upper"
title = "Upper"
[[contributes.commands]]
id = "echo.bytes"
title = "Bytes"
[[contributes.commands]]
id = "echo.read"
title = "Read"
[[contributes.commands]]
id = "echo.write"
title = "Write"
[[contributes.commands]]
id = "echo.spin"
title = "Spin"
[[contributes.commands]]
id = "echo.grow"
title = "Grow"
"#;

fn manifest(permissions: &str) -> Manifest {
    Manifest::from_toml(&format!("{BASE}\n{permissions}")).expect("valid manifest")
}

#[derive(Default)]
struct Doc(Mutex<String>);

impl BufferService for Doc {
    fn read_active(&self) -> Result<String, String> {
        Ok(self.0.lock().map_err(|e| e.to_string())?.clone())
    }
    fn write_active(&self, text: &str) -> Result<(), String> {
        *self.0.lock().map_err(|e| e.to_string())? = text.to_owned();
        Ok(())
    }
}

fn kernel(permissions: &str, limits: Limits, doc: Arc<Doc>) -> (Kernel, WasmRuntime) {
    let runtime = WasmRuntime::new(limits).expect("engine");
    let manifest = manifest(permissions);
    let plugin = runtime
        .load(&manifest, guest_component(), Some(doc))
        .expect("loads");
    let kernel = Kernel::new();
    kernel
        .register(manifest, Arc::new(plugin))
        .expect("registers");
    (kernel, runtime)
}

const RW: &str = "[permissions]\nbuffer = [\"read:active\", \"write:active\"]\n";
const RO: &str = "[permissions]\nbuffer = [\"read:active\"]\n";

fn failure(err: KernelError) -> String {
    err.to_string()
}

#[test]
fn payloads_round_trip_through_the_sandbox() {
    let (k, _rt) = kernel(RW, Limits::default(), Arc::default());
    assert_eq!(
        k.invoke("echo.say", Payload::Json(json!({"a": [1, 2]})))
            .expect("ok"),
        Payload::Json(json!({"a": [1, 2]}))
    );
    assert_eq!(
        k.invoke("echo.bytes", Payload::Binary(Arc::from(&[1u8, 2, 3][..])))
            .expect("ok"),
        Payload::Binary(Arc::from(&[3u8, 2, 1][..]))
    );
    assert!(failure(k.invoke("echo.nope", Payload::Empty).expect_err("unknown")).contains("nope"));
}

#[test]
fn buffer_access_follows_the_manifest() {
    let doc = Arc::new(Doc::default());
    let (k, _rt) = kernel(RW, Limits::default(), Arc::clone(&doc));
    k.invoke("echo.write", Payload::Json(json!("# Title")))
        .expect("write granted");
    assert_eq!(
        k.invoke("echo.read", Payload::Empty).expect("ok"),
        Payload::Json(json!("# Title"))
    );

    // Read-only grant: the import is linked, but writing is refused.
    let (k, _rt) = kernel(RO, Limits::default(), doc);
    k.invoke("echo.read", Payload::Empty).expect("read granted");
    let err = failure(
        k.invoke("echo.write", Payload::Json(json!("x")))
            .expect_err("denied"),
    );
    assert!(err.contains("lacks the capability"), "{err}");
}

#[test]
fn ungranted_import_is_rejected_at_load() {
    let runtime = WasmRuntime::new(Limits::default()).expect("engine");
    let err = runtime
        .load(&manifest(""), guest_component(), None)
        .err()
        .expect("must refuse");
    assert!(matches!(err, WasmError::UngrantedImport { .. }), "{err}");
}

#[test]
fn native_tier_manifests_are_not_loadable_as_wasm() {
    let runtime = WasmRuntime::new(Limits::default()).expect("engine");
    let native =
        Manifest::from_toml("[plugin]\nname = \"n\"\nversion = \"1.0.0\"\ntier = \"native\"\n")
            .expect("valid");
    assert!(matches!(
        runtime.load(&native, guest_component(), None).err(),
        Some(WasmError::WrongTier(_))
    ));
}

#[test]
fn runaway_plugin_is_stopped_by_the_deadline() {
    let limits = Limits {
        deadline: Duration::from_millis(100),
        ..Limits::default()
    };
    let (k, _rt) = kernel(RW, limits, Arc::default());
    let started = Instant::now();
    let err = failure(
        k.invoke("echo.spin", Payload::Empty)
            .expect_err("must trap"),
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "took {:?}",
        started.elapsed()
    );
    assert!(err.contains("trapped"), "{err}");
    // The host is still healthy afterwards.
    k.invoke("echo.say", Payload::Empty)
        .expect("still serves calls");
}

#[test]
fn memory_hog_is_stopped_by_the_cap() {
    let limits = Limits {
        memory_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    let (k, _rt) = kernel(RW, limits, Arc::default());
    let err = failure(
        k.invoke("echo.grow", Payload::Empty)
            .expect_err("must trap"),
    );
    assert!(err.contains("trapped"), "{err}");
    k.invoke("echo.say", Payload::Empty)
        .expect("still serves calls");
}
