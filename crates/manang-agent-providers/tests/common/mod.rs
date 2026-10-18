//! A small in-process HTTP server for the provider tests: serves scripted
//! replies over a real TCP socket, records requests, and reports when the
//! client closes the connection.
#![allow(dead_code)]

use futures_util::StreamExt;
use manang_agent_core::{ChatRequest, Message, Provider, ProviderEvent};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Notify};

#[derive(Debug, Clone)]
pub struct Captured {
    pub path: String,
    /// Lowercase names.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Captured {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// Written one by one with a short pause, to exercise chunk splitting.
    pub chunks: Vec<Vec<u8>>,
    /// Keep the connection open after the chunks and wait for the client to
    /// close it.
    pub hold: bool,
}

impl Reply {
    pub fn sse(body: &str) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".into(), "text/event-stream".into())],
            chunks: vec![body.as_bytes().to_vec()],
            hold: false,
        }
    }

    /// The same body cut into small pieces (splits inside frames and inside
    /// multi-byte characters).
    pub fn sse_split(body: &str, piece: usize) -> Self {
        let mut reply = Self::sse(body);
        reply.chunks = body.as_bytes().chunks(piece).map(<[u8]>::to_vec).collect();
        reply
    }

    pub fn status(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), "application/json".into())],
            chunks: vec![body.as_bytes().to_vec()],
            hold: false,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn held(mut self) -> Self {
        self.hold = true;
        self
    }
}

pub struct MockServer {
    pub addr: SocketAddr,
    pub requests: Arc<Mutex<Vec<Captured>>>,
    /// Notified when a held connection sees the client hang up.
    pub client_closed: Arc<Notify>,
}

impl MockServer {
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub async fn request_count(&self) -> usize {
        self.requests.lock().await.len()
    }

    pub async fn captured(&self, index: usize) -> Captured {
        self.requests.lock().await[index].clone()
    }
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        529 => "Overloaded",
        _ => "Status",
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Option<Captured> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut tmp).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.split("\r\n");
    let request_line = lines.next()?;
    let path = request_line.split(' ').nth(1)?.to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < header_end + length {
        let n = stream.read(&mut tmp).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    let body = String::from_utf8_lossy(&buf[header_end..]).into_owned();
    Some(Captured {
        path,
        headers,
        body,
    })
}

/// Starts a server on an ephemeral loopback port. Request N gets reply N;
/// after the script runs out the last reply repeats.
pub async fn serve(script: Vec<Reply>) -> MockServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let client_closed = Arc::new(Notify::new());
    let (reqs, closed) = (Arc::clone(&requests), Arc::clone(&client_closed));
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let Some(captured) = read_request(&mut stream).await else {
                continue;
            };
            let index = {
                let mut guard = reqs.lock().await;
                guard.push(captured);
                guard.len() - 1
            };
            let reply = script
                .get(index)
                .or_else(|| script.last())
                .cloned()
                .expect("script has a reply");
            let closed = Arc::clone(&closed);
            tokio::spawn(async move {
                let mut head = format!(
                    "HTTP/1.1 {} {}\r\nconnection: close\r\n",
                    reply.status,
                    status_text(reply.status)
                );
                for (k, v) in &reply.headers {
                    head.push_str(&format!("{k}: {v}\r\n"));
                }
                head.push_str("\r\n");
                if stream.write_all(head.as_bytes()).await.is_err() {
                    return;
                }
                for chunk in &reply.chunks {
                    if stream.write_all(chunk).await.is_err() || stream.flush().await.is_err() {
                        closed.notify_one();
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
                if reply.hold {
                    let mut tmp = [0u8; 64];
                    loop {
                        match stream.read(&mut tmp).await {
                            Ok(0) | Err(_) => break,
                            Ok(_) => {}
                        }
                    }
                    closed.notify_one();
                } else {
                    let _ = stream.shutdown().await;
                }
            });
        }
    });
    MockServer {
        addr,
        requests,
        client_closed,
    }
}

pub fn chat(model: &str, text: &str) -> ChatRequest {
    ChatRequest {
        model: model.to_owned(),
        system: Some("You are a careful scientific editor.".to_owned()),
        messages: vec![Message::user_text(text).expect("message")],
        tools: vec![],
        max_tokens: 1024,
        cache_breakpoints: vec![],
    }
}

pub async fn collect(provider: &dyn Provider, request: &ChatRequest) -> Vec<ProviderEvent> {
    tokio::time::timeout(
        Duration::from_secs(10),
        provider.stream(request).collect::<Vec<_>>(),
    )
    .await
    .expect("stream finished in time")
}
