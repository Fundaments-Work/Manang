//! Host hardening: every ceiling the sandbox promises, exercised against both
//! the on-demand and the pooling allocator.

use std::sync::Arc;
use std::time::{Duration, Instant};

use manang_kernel::{Kernel, Payload};
use manang_wasm_host::{BufferService, HostServices, Limits, RuntimeOptions, WasmRuntime};
use serde_json::json;

mod common;
use common::{echo_guest, manifest, wat_component, wat_plugin};

const ECHO: &str = r#"
[plugin]
name = "echo"
version = "0.1.0"
[permissions]
buffer = ["read:active", "write:active"]
"#;

fn echo_manifest() -> manang_kernel::Manifest {
    let mut src = ECHO.to_owned();
    for c in [
        "say", "read", "big", "bigjson", "badjson", "longerr", "trap", "stack",
    ] {
        src.push_str(&format!(
            "[[contributes.commands]]\nid = \"echo.{c}\"\ntitle = \"{c}\"\n"
        ));
    }
    manifest(&src)
}

fn runtime(limits: Limits, pooling: bool) -> WasmRuntime {
    WasmRuntime::with_options(RuntimeOptions {
        limits,
        pooling,
        pool_slots: 8,
        cache_dir: None,
    })
    .expect("engine")
}

fn echo_kernel(limits: Limits, pooling: bool, services: HostServices) -> (Kernel, WasmRuntime) {
    let rt = runtime(limits, pooling);
    assert_eq!(rt.pooling(), pooling, "requested allocator is active");
    let m = echo_manifest();
    let plugin = rt.load_with(&m, echo_guest(), services).expect("loads");
    let k = Kernel::new();
    k.register(m, Arc::new(plugin)).expect("registers");
    (k, rt)
}

fn err(k: &Kernel, command: &str, input: Payload) -> String {
    k.invoke(command, input).expect_err("must fail").to_string()
}

#[test]
fn payload_caps_apply_in_both_directions() {
    for pooling in [false, true] {
        let limits = Limits {
            max_payload_bytes: 1024,
            max_output_bytes: 1024,
            ..Limits::default()
        };
        let (k, _rt) = echo_kernel(limits, pooling, HostServices::default());
        let big = Payload::Binary(Arc::from(vec![0u8; 2048]));
        assert!(err(&k, "echo.say", big).contains("payload limit"));
        let ok = Payload::Binary(Arc::from(vec![0u8; 512]));
        assert!(k.invoke("echo.say", ok).is_ok());

        assert!(err(&k, "echo.big", Payload::Json(json!(4096))).contains("output limit"));
        assert!(k.invoke("echo.big", Payload::Json(json!(512))).is_ok());
        assert!(err(&k, "echo.bigjson", Payload::Json(json!(4096))).contains("output limit"));
        assert!(err(&k, "echo.badjson", Payload::Empty).contains("invalid JSON"));
    }
}

#[test]
fn guest_error_text_is_truncated() {
    let (k, _rt) = echo_kernel(Limits::default(), false, HostServices::default());
    let message = err(&k, "echo.longerr", Payload::Empty);
    assert!(message.len() < 5000, "{} bytes", message.len());
    assert!(message.contains("truncated"));
}

#[test]
fn guest_traps_and_stack_overflow_do_not_hurt_the_host() {
    for pooling in [false, true] {
        let (k, _rt) = echo_kernel(Limits::default(), pooling, HostServices::default());
        assert!(err(&k, "echo.trap", Payload::Empty).contains("trapped"));
        assert!(err(&k, "echo.stack", Payload::Empty).contains("trapped"));
        k.invoke("echo.say", Payload::Empty).expect("still healthy");
    }
}

struct Panicky;

impl BufferService for Panicky {
    fn read_active(&self) -> Result<String, String> {
        panic!("buffer service exploded")
    }
    fn write_active(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn a_panicking_host_service_is_contained_at_the_boundary() {
    for pooling in [false, true] {
        let services = HostServices {
            buffer: Some(Arc::new(Panicky)),
            ..HostServices::default()
        };
        let (k, _rt) = echo_kernel(Limits::default(), pooling, services);
        let message = err(&k, "echo.read", Payload::Empty);
        assert!(message.contains("panicked"), "{message}");
        assert!(message.contains("buffer service exploded"), "{message}");
        k.invoke("echo.say", Payload::Empty)
            .expect("host survives and serves the next call");
    }
}

fn wat_kernel(wat: &str, limits: Limits, pooling: bool) -> (Kernel, WasmRuntime) {
    let rt = runtime(limits, pooling);
    let m = manifest(
        "[plugin]\nname = \"wat\"\nversion = \"0.1.0\"\n[[contributes.commands]]\nid = \"wat.run\"\ntitle = \"run\"\n",
    );
    let plugin = rt
        .load(&m, &wat_component(wat), None)
        .expect("wat component loads");
    let k = Kernel::new();
    k.register(m, Arc::new(plugin)).expect("registers");
    (k, rt)
}

#[test]
fn instantiation_is_covered_by_the_deadline() {
    // A start function that never returns runs during instantiation, before
    // any exported function is called.
    let wat = wat_plugin("(func $spin (loop $l br $l))\n(start $spin)", "");
    for pooling in [false, true] {
        let limits = Limits {
            deadline: Duration::from_millis(100),
            ..Limits::default()
        };
        let (k, _rt) = wat_kernel(&wat, limits, pooling);
        let started = Instant::now();
        let message = err(&k, "wat.run", Payload::Empty);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "took {:?}",
            started.elapsed()
        );
        assert!(message.contains("instantiation failed"), "{message}");
    }
}

#[test]
fn tables_are_capped() {
    let grow = |n: u32| {
        wat_plugin(
            "(table $t 1 funcref)",
            &format!(
                "ref.null func\n i32.const {n}\n table.grow $t\n i32.const -1\n i32.eq\n if unreachable end"
            ),
        )
    };
    for pooling in [false, true] {
        let limits = Limits {
            table_elements: 1000,
            ..Limits::default()
        };
        let (k, _rt) = wat_kernel(&grow(100), limits, pooling);
        k.invoke("wat.run", Payload::Empty).expect("small grow ok");
        let (k, _rt) = wat_kernel(&grow(10_000_000), limits, pooling);
        assert!(err(&k, "wat.run", Payload::Empty).contains("trapped"));
        k.invoke("wat.run", Payload::Empty)
            .expect_err("still capped on the next call");
    }
}

#[test]
fn oversized_initial_memory_is_refused() {
    // 2000 pages = 125 MiB, over a 16 MiB cap.
    let wat = wat_plugin("", "").replace(
        "(memory (export \"memory\") 1)",
        "(memory (export \"memory\") 2000)",
    );
    let limits = Limits {
        memory_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    // On-demand: refused when instantiated. Pooling: refused already at load.
    let (k, _rt) = wat_kernel(&wat, limits, false);
    assert!(err(&k, "wat.run", Payload::Empty).contains("instantiation failed"));
    let rt = runtime(limits, true);
    let m = manifest("[plugin]\nname = \"wat\"\nversion = \"0.1.0\"\n");
    assert!(rt.load(&m, &wat_component(&wat), None).is_err());
}

#[test]
fn memory_growth_during_instantiation_is_capped() {
    let wat = wat_plugin("(func $s i32.const 5000 memory.grow drop)\n(start $s)", "");
    for pooling in [false, true] {
        let limits = Limits {
            memory_bytes: 16 * 1024 * 1024,
            ..Limits::default()
        };
        let (k, _rt) = wat_kernel(&wat, limits, pooling);
        assert!(err(&k, "wat.run", Payload::Empty).contains("instantiation failed"));
    }
}

#[test]
fn concurrent_calls_beyond_the_pool_fail_cleanly_not_hang() {
    // Not a correctness guarantee about *which* calls fail: only that heavy
    // parallel use never deadlocks or crashes the host.
    let (k, _rt) = echo_kernel(Limits::default(), true, HostServices::default());
    let k = Arc::new(k);
    let handles: Vec<_> = (0..32)
        .map(|_| {
            let k = Arc::clone(&k);
            std::thread::spawn(move || {
                (0..20)
                    .filter(|_| k.invoke("echo.say", Payload::Empty).is_ok())
                    .count()
            })
        })
        .collect();
    let ok: usize = handles
        .into_iter()
        .map(|h| h.join().expect("no panic"))
        .sum();
    assert!(ok > 0);
    k.invoke("echo.say", Payload::Empty)
        .expect("healthy after the storm");
}
