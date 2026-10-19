//! Test support: a local HTTP server standing in for the registry host, and
//! helpers to build and sign an index with a throwaway key. Compiled for this
//! crate's tests and, with the `testing` feature, for other crates' tests.
//! Never part of the app.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signer, SigningKey};
use parking_lot_free::Mutex;

use crate::keys::TrustedKeys;

/// std's mutex with the poisoning unwrapped: test code only.
mod parking_lot_free {
    pub struct Mutex<T>(std::sync::Mutex<T>);
    impl<T> Mutex<T> {
        pub fn new(v: T) -> Self {
            Self(std::sync::Mutex::new(v))
        }
        pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
            self.0.lock().unwrap_or_else(|e| e.into_inner())
        }
    }
}

/// A throwaway signing key from a one-byte seed.
pub fn test_signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

/// The key list that trusts only `key`.
pub fn trusting(key: &SigningKey) -> TrustedKeys {
    TrustedKeys::from_keys(vec![key.verifying_key()])
}

/// Base64 signature of `bytes`, as `index.json.sig` holds it.
pub fn sign(key: &SigningKey, bytes: &[u8]) -> String {
    STANDARD.encode(key.sign(bytes).to_bytes())
}

#[derive(Clone)]
struct File {
    body: Vec<u8>,
    etag: Option<String>,
    /// Answer with this status instead of the file.
    status: Option<u16>,
}

#[derive(Default)]
struct State {
    files: HashMap<String, File>,
    requests: Vec<(String, bool)>,
}

/// Serves files from memory on 127.0.0.1. Understands `If-None-Match`.
pub struct TestServer {
    addr: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TestServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = format!("http://{}", listener.local_addr().expect("addr"));
        listener.set_nonblocking(true).expect("nonblocking");
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, st) = (Arc::clone(&state), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            while !st.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let s = Arc::clone(&s);
                        std::thread::spawn(move || serve(stream, &s));
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                }
            }
        });
        Self {
            addr,
            state,
            stop,
            thread: Some(thread),
        }
    }

    pub fn url(&self) -> &str {
        &self.addr
    }

    /// Serve `body` at `/path` (with an ETag derived from it).
    pub fn put(&self, path: &str, body: impl Into<Vec<u8>>) {
        let body = body.into();
        let etag = Some(format!("\"{}\"", crate::sha256_hex(&body)[..16].to_owned()));
        self.state.lock().files.insert(
            path.to_owned(),
            File {
                body,
                etag,
                status: None,
            },
        );
    }

    /// Answer `/path` with a bare HTTP `status`.
    pub fn fail(&self, path: &str, status: u16) {
        self.state.lock().files.insert(
            path.to_owned(),
            File {
                body: Vec::new(),
                etag: None,
                status: Some(status),
            },
        );
    }

    pub fn remove(&self, path: &str) {
        self.state.lock().files.remove(path);
    }

    /// Paths requested so far, with whether the request was conditional.
    pub fn requests(&self) -> Vec<(String, bool)> {
        self.state.lock().requests.clone()
    }

    pub fn count(&self, path: &str) -> usize {
        self.requests().iter().filter(|(p, _)| p == path).count()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn serve(stream: TcpStream, state: &Mutex<State>) {
    let _ = stream.set_nonblocking(false);
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();
    let mut if_none_match = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && k.eq_ignore_ascii_case("if-none-match")
        {
            if_none_match = Some(v.trim().to_owned());
        }
    }
    let file = {
        let mut s = state.lock();
        s.requests.push((path.clone(), if_none_match.is_some()));
        s.files.get(path.trim_start_matches('/')).cloned()
    };
    let mut out = &stream;
    let _ = match file {
        None => out
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
        Some(File {
            status: Some(code), ..
        }) => out.write_all(
            format!("HTTP/1.1 {code} Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        ),
        Some(f) if f.etag.is_some() && f.etag == if_none_match => out.write_all(
            format!(
                "HTTP/1.1 304 Not Modified\r\nETag: {}\r\nConnection: close\r\n\r\n",
                f.etag.unwrap_or_default()
            )
            .as_bytes(),
        ),
        Some(f) => {
            let mut head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n",
                f.body.len()
            );
            if let Some(e) = &f.etag {
                head.push_str(&format!("ETag: {e}\r\n"));
            }
            head.push_str("\r\n");
            out.write_all(head.as_bytes())
                .and_then(|()| out.write_all(&f.body))
        }
    };
}
