//! Per-call overhead of the WASM tier and cold-start cost.
//!
//! `cargo bench -p manang-wasm-host --bench call_overhead`
//!
//! Every call builds a fresh `Store`, instantiates the component and runs the
//! guest, so these numbers are the full price of statelessness. The budget for
//! the trivial call is ~100 microseconds.

use std::sync::Arc;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use manang_kernel::{Kernel, Payload};
use manang_wasm_host::{Limits, RuntimeOptions, WasmRuntime};
use serde_json::json;

#[path = "../tests/common/mod.rs"]
mod common;

fn kernel(pooling: bool) -> (Kernel, WasmRuntime) {
    let rt = WasmRuntime::with_options(RuntimeOptions {
        limits: Limits::default(),
        pooling,
        pool_slots: 16,
        cache_dir: None,
    })
    .expect("engine");
    let m = common::manifest(
        "[plugin]\nname = \"echo\"\nversion = \"0.1.0\"\n\
         [permissions]\nbuffer = [\"read:active\"]\n\
         [[contributes.commands]]\nid = \"echo.say\"\ntitle = \"say\"\n",
    );
    let plugin = rt
        .load(&m, common::echo_guest(), None)
        .expect("component loads");
    let k = Kernel::new();
    k.register(m, Arc::new(plugin)).expect("registers");
    (k, rt)
}

fn calls(c: &mut Criterion) {
    let json_1k = Payload::Json(json!("x".repeat(1000)));
    let binary_1m = Payload::Binary(Arc::from(vec![7u8; 1024 * 1024]));
    for (label, pooling) in [("ondemand", false), ("pooling", true)] {
        let (k, _rt) = kernel(pooling);
        let mut group = c.benchmark_group(format!("call/{label}"));
        group.bench_function("empty", |b| {
            b.iter(|| k.invoke("echo.say", Payload::Empty).expect("ok"));
        });
        group.throughput(Throughput::Bytes(1000));
        group.bench_function("json_1kb", |b| {
            b.iter(|| k.invoke("echo.say", json_1k.clone()).expect("ok"));
        });
        group.throughput(Throughput::Bytes(1024 * 1024));
        group.bench_function("binary_1mb", |b| {
            b.iter(|| k.invoke("echo.say", binary_1m.clone()).expect("ok"));
        });
        group.finish();
    }
}

fn cold_start(c: &mut Criterion) {
    let wasm = common::echo_guest();
    let m = common::manifest(
        "[plugin]\nname = \"echo\"\nversion = \"0.1.0\"\n[permissions]\nbuffer = [\"read:active\"]\n",
    );
    let mut group = c.benchmark_group("load");
    group.sample_size(10);

    let rt = WasmRuntime::new(Limits::default()).expect("engine");
    group.bench_function("compile", |b| {
        b.iter(|| rt.load(&m, wasm, None).expect("loads"));
    });

    let dir = tempfile::tempdir().expect("tempdir");
    let cached = WasmRuntime::with_options(RuntimeOptions {
        cache_dir: Some(dir.path().to_path_buf()),
        pooling: false,
        ..RuntimeOptions::default()
    })
    .expect("engine");
    cached.load(&m, wasm, None).expect("warm the cache");
    group.bench_function("cache_hit", |b| {
        b.iter(|| cached.load(&m, wasm, None).expect("loads"));
    });
    group.finish();
}

criterion_group!(benches, calls, cold_start);
criterion_main!(benches);
