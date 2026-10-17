//! Hot reload driven by real file-system events.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use manang_kernel::{Kernel, Payload};
use manang_plugins::{ManagerConfig, PluginEvent, PluginManager, WatchHandle, WatchOptions};
use serde_json::json;

mod common;
use common::{component, install};

const WAIT: Duration = Duration::from_secs(10);

fn start(
    plugins: &std::path::Path,
) -> (
    Arc<Kernel>,
    Arc<PluginManager>,
    WatchHandle,
    Receiver<PluginEvent>,
) {
    let kernel = Arc::new(Kernel::new());
    let manager = Arc::new(
        PluginManager::new(Arc::clone(&kernel), ManagerConfig::new(plugins)).expect("manager"),
    );
    manager.scan().expect("initial scan");
    let (tx, rx) = channel();
    let handle = manager
        .watch(
            WatchOptions {
                debounce: Duration::from_millis(150),
                max_wait: Duration::from_secs(2),
            },
            move |event| {
                let _ = tx.send(event);
            },
        )
        .expect("watch");
    (kernel, manager, handle, rx)
}

/// Wait for the first event matching `pick`, discarding others.
fn expect<T>(rx: &Receiver<PluginEvent>, pick: impl Fn(&PluginEvent) -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if let Ok(event) = rx.recv_timeout(left) {
            if let Some(found) = pick(&event) {
                return found;
            }
        }
    }
    panic!("timed out waiting for a plugin event");
}

fn answer(kernel: &Kernel, name: &str) -> Payload {
    kernel
        .invoke(&format!("{name}.run"), Payload::Empty)
        .expect("runs")
}

#[test]
fn new_changed_and_removed_plugins_are_picked_up() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (kernel, _manager, _watch, rx) = start(tmp.path());

    // New plugin appears.
    let dir = install(tmp.path(), "alpha", &component('1', 1));
    expect(&rx, |e| {
        matches!(e, PluginEvent::Loaded(i) if i.name == "alpha").then_some(())
    });
    assert_eq!(answer(&kernel, "alpha"), Payload::Json(json!(1)));

    // Its wasm is replaced.
    std::fs::write(dir.join("plugin.wasm"), component('2', 1)).expect("write v2");
    expect(&rx, |e| {
        matches!(e, PluginEvent::Reloaded(i) if i.name == "alpha").then_some(())
    });
    assert_eq!(answer(&kernel, "alpha"), Payload::Json(json!(2)));

    // A broken update is reported; v2 keeps serving.
    std::fs::write(dir.join("plugin.wasm"), b"garbage").expect("write junk");
    expect(&rx, |e| match e {
        PluginEvent::Failed(f) if f.plugin.as_deref() == Some("alpha") => Some(()),
        _ => None,
    });
    assert_eq!(answer(&kernel, "alpha"), Payload::Json(json!(2)));

    // Deleting the directory unloads it.
    std::fs::remove_dir_all(&dir).expect("remove");
    expect(&rx, |e| {
        matches!(e, PluginEvent::Unloaded(n) if n == "alpha").then_some(())
    });
    assert!(kernel.invoke("alpha.run", Payload::Empty).is_err());
}

#[test]
fn a_burst_of_writes_causes_one_reload() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = install(tmp.path(), "alpha", &component('1', 1));
    let (kernel, _manager, _watch, rx) = start(tmp.path());

    // Simulate a build tool rewriting the file many times in quick succession.
    for _ in 0..25 {
        std::fs::write(dir.join("plugin.wasm"), component('5', 1)).expect("write");
        std::thread::sleep(Duration::from_millis(5));
    }
    expect(&rx, |e| matches!(e, PluginEvent::Reloaded(_)).then_some(()));
    // Give any (wrong) extra reload time to show up.
    std::thread::sleep(Duration::from_millis(600));
    let extra = std::iter::from_fn(|| rx.try_recv().ok())
        .filter(|e| matches!(e, PluginEvent::Reloaded(_)))
        .count();
    assert_eq!(extra, 0, "debounce must coalesce the burst");
    assert_eq!(answer(&kernel, "alpha"), Payload::Json(json!(5)));
}

#[test]
fn dropping_the_handle_stops_watching() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (kernel, _manager, watch, rx) = start(tmp.path());
    drop(watch);
    install(tmp.path(), "alpha", &component('1', 1));
    assert!(rx.recv_timeout(Duration::from_millis(800)).is_err());
    assert!(kernel.invoke("alpha.run", Payload::Empty).is_err());
}
