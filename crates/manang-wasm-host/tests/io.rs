//! The `fs` and `http` interfaces, end to end through a real guest component.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{Kernel, Payload};
use manang_wasm_host::{HostServices, Limits, ProjectRoot, WasmError, WasmRuntime};
use serde_json::{Value, json};

mod common;
use common::{io_guest, manifest};

fn manifest_with(permissions: &str) -> manang_kernel::Manifest {
    let mut src = format!("[plugin]\nname = \"io\"\nversion = \"0.1.0\"\n{permissions}\n");
    for c in ["read", "write", "list", "get"] {
        src.push_str(&format!(
            "[[contributes.commands]]\nid = \"io.{c}\"\ntitle = \"{c}\"\n"
        ));
    }
    manifest(&src)
}

const ALL: &str =
    "[permissions]\nfilesystem = [\"read:templates\", \"write:out\"]\nnetwork = [\"127.0.0.1\"]\n";

fn setup(limits: Limits, project: Option<&std::path::Path>) -> (Kernel, WasmRuntime) {
    let rt = WasmRuntime::new(limits).expect("engine");
    let m = manifest_with(ALL);
    let services = HostServices {
        project: Arc::new(ProjectRoot::new(project.map(Into::into))),
        ..HostServices::default()
    };
    let plugin = rt.load_with(&m, io_guest(), services).expect("loads");
    let k = Kernel::new();
    k.register(m, Arc::new(plugin)).expect("registers");
    (k, rt)
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path();
    std::fs::create_dir_all(p.join("templates")).expect("mkdir");
    std::fs::create_dir_all(p.join("secret")).expect("mkdir");
    std::fs::write(p.join("templates/a.typ"), b"hello").expect("write");
    std::fs::write(p.join("secret/key"), b"s3cret").expect("write");
    dir
}

fn call(k: &Kernel, command: &str, args: Value) -> Result<Payload, String> {
    k.invoke(command, Payload::Json(args))
        .map_err(|e| e.to_string())
}

#[test]
fn fs_follows_the_manifest_scopes() {
    let dir = project();
    let (k, _rt) = setup(Limits::default(), Some(dir.path()));

    assert_eq!(
        call(&k, "io.read", json!({"path": "templates/a.typ"})).expect("read"),
        Payload::Binary(Arc::from(&b"hello"[..]))
    );
    call(
        &k,
        "io.write",
        json!({"path": "out/x/y.txt", "data": "made"}),
    )
    .expect("write");
    assert_eq!(
        std::fs::read(dir.path().join("out/x/y.txt")).expect("exists"),
        b"made"
    );
    let listing = call(&k, "io.list", json!({"path": "templates"})).expect("list");
    assert_eq!(
        listing,
        Payload::Json(json!([{"name": "a.typ", "dir": false, "size": 5}]))
    );

    for path in [
        "secret/key",
        "templates/../secret/key",
        "/etc/passwd",
        "..",
        "templates\\..\\secret\\key",
    ] {
        let message = call(&k, "io.read", json!({ "path": path })).expect_err(path);
        assert!(!message.contains("s3cret"));
    }
    // Read-only scope cannot be written; write scope can be read.
    assert!(
        call(
            &k,
            "io.write",
            json!({"path": "templates/b.typ", "data": "x"})
        )
        .is_err()
    );
    assert!(!dir.path().join("templates/b.typ").exists());
    assert!(call(&k, "io.read", json!({"path": "out/x/y.txt"})).is_ok());
}

#[cfg(unix)]
#[test]
fn fs_refuses_symlink_escapes_through_the_sandbox() {
    let dir = project();
    std::os::unix::fs::symlink(dir.path().join("secret"), dir.path().join("templates/link"))
        .expect("symlink");
    std::os::unix::fs::symlink("../secret/key", dir.path().join("templates/keylink"))
        .expect("symlink");
    let (k, _rt) = setup(Limits::default(), Some(dir.path()));
    assert!(call(&k, "io.read", json!({"path": "templates/link/key"})).is_err());
    assert!(call(&k, "io.read", json!({"path": "templates/keylink"})).is_err());
    assert!(call(&k, "io.write", json!({"path": "out/../x", "data": "x"})).is_err());
}

#[test]
fn fs_without_an_open_project_fails() {
    let (k, _rt) = setup(Limits::default(), None);
    let message = call(&k, "io.read", json!({"path": "templates/a.typ"})).expect_err("no project");
    assert!(message.contains("no project is open"), "{message}");
}

#[test]
fn fs_project_can_change_without_reloading() {
    let a = project();
    let b = project();
    std::fs::write(b.path().join("templates/a.typ"), b"from b").expect("write");
    let rt = WasmRuntime::new(Limits::default()).expect("engine");
    let root = Arc::new(ProjectRoot::new(Some(a.path().to_path_buf())));
    let m = manifest_with(ALL);
    let plugin = rt
        .load_with(
            &m,
            io_guest(),
            HostServices {
                project: Arc::clone(&root),
                ..HostServices::default()
            },
        )
        .expect("loads");
    let k = Kernel::new();
    k.register(m, Arc::new(plugin)).expect("registers");
    let read = || call(&k, "io.read", json!({"path": "templates/a.typ"})).expect("read");
    assert_eq!(read(), Payload::Binary(Arc::from(&b"hello"[..])));
    root.set(Some(b.path().to_path_buf()));
    assert_eq!(read(), Payload::Binary(Arc::from(&b"from b"[..])));
}

#[test]
fn fs_file_size_limit_applies() {
    let dir = project();
    std::fs::write(dir.path().join("templates/big"), vec![0u8; 4096]).expect("write");
    let limits = Limits {
        max_fs_file_bytes: 1024,
        ..Limits::default()
    };
    let (k, _rt) = setup(limits, Some(dir.path()));
    assert!(call(&k, "io.read", json!({"path": "templates/big"})).is_err());
    assert!(call(&k, "io.read", json!({"path": "templates/a.typ"})).is_ok());
}

fn serve_once(delay: Duration, body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            std::thread::sleep(delay);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    port
}

#[test]
fn http_is_limited_to_granted_hosts() {
    let dir = project();
    let (k, _rt) = setup(Limits::default(), Some(dir.path()));
    let port = serve_once(Duration::ZERO, "pong");
    let ok = call(
        &k,
        "io.get",
        json!({"url": format!("http://127.0.0.1:{port}/")}),
    )
    .expect("ok");
    assert_eq!(ok, Payload::Json(json!({"status": 200, "body": "pong"})));

    let denied = call(&k, "io.get", json!({"url": "http://localhost:1/"})).expect_err("denied");
    assert!(denied.contains("network grants"), "{denied}");
}

#[test]
fn host_io_time_is_not_charged_to_the_guest_deadline() {
    let dir = project();
    let limits = Limits {
        deadline: Duration::from_millis(100),
        ..Limits::default()
    };
    let (k, _rt) = setup(limits, Some(dir.path()));
    // The server takes three times the guest deadline to answer.
    let port = serve_once(Duration::from_millis(300), "slow");
    let ok = call(
        &k,
        "io.get",
        json!({"url": format!("http://127.0.0.1:{port}/")}),
    );
    assert_eq!(
        ok,
        Ok(Payload::Json(json!({"status": 200, "body": "slow"})))
    );
}

#[test]
fn http_timeout_bounds_a_hanging_server() {
    let dir = project();
    let limits = Limits {
        http_timeout: Duration::from_millis(300),
        ..Limits::default()
    };
    let (k, _rt) = setup(limits, Some(dir.path()));
    let port = serve_once(Duration::from_secs(5), "never");
    let started = std::time::Instant::now();
    assert!(
        call(
            &k,
            "io.get",
            json!({"url": format!("http://127.0.0.1:{port}/")})
        )
        .is_err()
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn imports_need_matching_grants() {
    let rt = WasmRuntime::new(Limits::default()).expect("engine");
    // The guest imports fs and http: grant only the network.
    let m = manifest_with("[permissions]\nnetwork = [\"127.0.0.1\"]\n");
    let err = rt.load(&m, io_guest(), None).err().expect("refused");
    assert!(
        matches!(&err, WasmError::UngrantedImport { interface, .. } if interface.contains("fs")),
        "{err}"
    );
    let m = manifest_with("[permissions]\nfilesystem = [\"read:templates\"]\n");
    let err = rt.load(&m, io_guest(), None).err().expect("refused");
    assert!(
        matches!(&err, WasmError::UngrantedImport { interface, .. } if interface.contains("http")),
        "{err}"
    );
}
