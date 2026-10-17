//! Allowlisted outbound HTTP for plugins.
//!
//! * The host of the request and of **every redirect hop** must exactly match
//!   a granted `network` host (ASCII case-insensitive, no wildcards, ports
//!   ignored). The client itself never follows redirects, so the check cannot
//!   be skipped.
//! * URLs carrying credentials, non-HTTP(S) schemes and IPv6 literals are
//!   refused. Ambient proxy settings are ignored.
//! * Request bodies, response bodies, redirect count and total wall-clock time
//!   are capped.

use std::time::{Duration, Instant};

use manang_kernel::CapabilitySet;
use ureq::http::{self, Method, Uri};

use crate::limits::Limits;

const MAX_REDIRECTS: usize = 5;
const MAX_HEADERS: usize = 64;
const MAX_URL_BYTES: usize = 8 * 1024;

/// Headers the host owns; a plugin cannot set them.
const FORBIDDEN_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "upgrade",
    "te",
    "trailer",
    "expect",
    "proxy-authorization",
    "proxy-connection",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

fn parse_method(method: &str) -> Result<Method, String> {
    match method {
        "GET" => Ok(Method::GET),
        "HEAD" => Ok(Method::HEAD),
        "POST" => Ok(Method::POST),
        "PUT" => Ok(Method::PUT),
        "DELETE" => Ok(Method::DELETE),
        other => Err(format!("method `{other}` is not allowed")),
    }
}

/// Parse `url` and check scheme, credentials and host against the grants.
fn authorise(url: &str, capabilities: &CapabilitySet) -> Result<Uri, String> {
    if url.len() > MAX_URL_BYTES {
        return Err("URL is too long".to_owned());
    }
    let uri: Uri = url.parse().map_err(|e| format!("invalid URL: {e}"))?;
    match uri.scheme_str() {
        Some("http" | "https") => {}
        _ => return Err("only http and https URLs are allowed".to_owned()),
    }
    let authority = uri.authority().ok_or("URL has no host")?;
    if authority.as_str().contains('@') {
        return Err("URLs with credentials are not allowed".to_owned());
    }
    let host = authority.host();
    if !capabilities.allows_host(host) {
        return Err(format!(
            "host `{host}` is not in the plugin's network grants"
        ));
    }
    Ok(uri)
}

fn validate_headers(headers: &[(String, String)]) -> Result<(), String> {
    if headers.len() > MAX_HEADERS {
        return Err(format!("more than {MAX_HEADERS} request headers"));
    }
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
        if FORBIDDEN_HEADERS.contains(&lower.as_str()) {
            return Err(format!("header `{name}` is managed by the host"));
        }
        http::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| format!("invalid header name `{name}`"))?;
        http::HeaderValue::from_str(value)
            .map_err(|_| format!("invalid value for header `{name}`"))?;
    }
    Ok(())
}

/// Resolve a `Location` header against the URL it came from.
fn resolve_location(base: &Uri, location: &str) -> Result<String, String> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Ok(location.to_owned());
    }
    let scheme = base
        .scheme_str()
        .ok_or("redirect from a URL without scheme")?;
    let authority = base.authority().ok_or("redirect from a URL without host")?;
    if location.starts_with("//") {
        return Ok(format!("{scheme}:{location}"));
    }
    if location.starts_with('/') {
        return Ok(format!("{scheme}://{authority}{location}"));
    }
    let path = base.path();
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    Ok(format!("{scheme}://{authority}{dir}/{location}"))
}

/// Perform a request under the plugin's grants and the host's limits.
///
/// `timeout` bounds the whole exchange including redirects.
pub(crate) fn fetch(
    capabilities: &CapabilitySet,
    request: &Request,
    limits: &Limits,
    timeout: Duration,
) -> Result<Response, String> {
    let mut method = parse_method(&request.method)?;
    validate_headers(&request.headers)?;
    let mut body = request.body.clone();
    if let Some(b) = &body {
        if b.len() > limits.max_http_body_bytes {
            return Err(format!(
                "request body of {} bytes exceeds the {} byte limit",
                b.len(),
                limits.max_http_body_bytes
            ));
        }
        if matches!(method, Method::GET | Method::HEAD | Method::DELETE) {
            return Err(format!("{method} requests cannot carry a body"));
        }
    }

    let started = Instant::now();
    let mut url = request.url.clone();
    for _hop in 0..=MAX_REDIRECTS {
        let uri = authorise(&url, capabilities)?;
        let remaining = timeout
            .checked_sub(started.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or("request timed out")?;

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .proxy(None)
            .timeout_global(Some(remaining))
            .user_agent("manang-plugin-host")
            .build()
            .into();
        let mut builder = http::Request::builder()
            .method(method.clone())
            .uri(uri.clone());
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let outgoing = builder
            .body(body.clone().unwrap_or_default())
            .map_err(|e| format!("invalid request: {e}"))?;
        let mut response = agent
            .run(outgoing)
            .map_err(|e| format!("request failed: {e}"))?;

        let status = response.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            if let Some(location) = response
                .headers()
                .get(http::header::LOCATION)
                .and_then(|v| v.to_str().ok())
            {
                url = resolve_location(&uri, location)?;
                // 303, and 301/302 on POST, become GET without a body.
                if status == 303 || (matches!(status, 301 | 302) && method == Method::POST) {
                    method = Method::GET;
                    body = None;
                }
                continue;
            }
        }

        let headers = response
            .headers()
            .iter()
            .take(MAX_HEADERS)
            .filter_map(|(n, v)| Some((n.as_str().to_owned(), v.to_str().ok()?.to_owned())))
            .collect();
        let data = response
            .body_mut()
            .with_config()
            .limit(limits.max_http_body_bytes as u64)
            .read_to_vec()
            .map_err(|e| format!("cannot read the response body: {e}"))?;
        return Ok(Response {
            status,
            headers,
            body: data,
        });
    }
    Err(format!("more than {MAX_REDIRECTS} redirects"))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use manang_kernel::Manifest;

    use super::*;

    fn caps(hosts: &[&str]) -> CapabilitySet {
        let list = hosts
            .iter()
            .map(|h| format!("\"{h}\""))
            .collect::<Vec<_>>()
            .join(",");
        Manifest::from_toml(&format!(
            "[plugin]\nname = \"t\"\nversion = \"0.1.0\"\n[permissions]\nnetwork = [{list}]\n"
        ))
        .expect("manifest")
        .capabilities()
        .expect("caps")
    }

    /// Read one full request (headers and any `Content-Length` body).
    fn read_request(stream: &mut TcpStream) -> String {
        let mut data = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = stream.read(&mut chunk).unwrap_or(0);
            if n == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&data);
            if let Some(end) = text.find("\r\n\r\n") {
                let length = text[..end]
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                    })
                    .unwrap_or(0);
                if data.len() >= end + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).into_owned()
    }

    /// Serve `handler(request_text)` for each of `n` connections.
    fn serve(n: usize, handler: impl Fn(&str) -> Vec<u8> + Send + 'static) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        thread::spawn(move || {
            for _ in 0..n {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let text = read_request(&mut stream);
                let _ = stream.write_all(&handler(&text));
                let _ = stream.flush();
                drop::<TcpStream>(stream);
            }
        });
        port
    }

    fn ok(body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn redirect(location: &str) -> Vec<u8> {
        format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .into_bytes()
    }

    fn get(url: String) -> Request {
        Request {
            method: "GET".into(),
            url,
            headers: vec![],
            body: None,
        }
    }

    const T: Duration = Duration::from_secs(5);

    #[test]
    fn fetches_from_a_granted_host() {
        let port = serve(1, |req| {
            assert!(req.starts_with("GET /hi HTTP/1.1"), "{req}");
            ok("hello")
        });
        let resp = fetch(
            &caps(&["127.0.0.1"]),
            &get(format!("http://127.0.0.1:{port}/hi")),
            &Limits::default(),
            T,
        )
        .expect("fetch");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"hello");
    }

    #[test]
    fn ungranted_hosts_and_bad_urls_are_refused_before_any_connection() {
        let c = caps(&["127.0.0.1"]);
        let l = Limits::default();
        for url in [
            "http://localhost:1/",
            "http://127.0.0.1.evil.org/",
            "http://evil.org@127.0.0.1:1/",
            "http://user:pw@127.0.0.1:1/",
            "ftp://127.0.0.1/",
            "file:///etc/passwd",
            "http://[::1]:1/",
            "/relative",
            "",
        ] {
            assert!(fetch(&c, &get(url.into()), &l, T).is_err(), "{url}");
        }
        assert!(fetch(&caps(&[]), &get("http://127.0.0.1:1/".into()), &l, T).is_err());
    }

    #[test]
    fn redirects_to_ungranted_hosts_are_refused() {
        let port = serve(1, |_| redirect("http://localhost:9/steal"));
        let err = fetch(
            &caps(&["127.0.0.1"]),
            &get(format!("http://127.0.0.1:{port}/")),
            &Limits::default(),
            T,
        )
        .expect_err("must refuse");
        assert!(err.contains("not in the plugin's network grants"), "{err}");
    }

    #[test]
    fn redirects_between_granted_hosts_are_followed() {
        let target = serve(1, |_| ok("landed"));
        let first = serve(1, move |_| {
            redirect(&format!("http://localhost:{target}/final"))
        });
        let resp = fetch(
            &caps(&["127.0.0.1", "localhost"]),
            &get(format!("http://127.0.0.1:{first}/")),
            &Limits::default(),
            T,
        )
        .expect("follows");
        assert_eq!(resp.body, b"landed");

        // Relative redirects stay on the granted host.
        let rel = serve(2, |req| {
            if req.starts_with("GET /start") {
                redirect("next")
            } else {
                assert!(req.starts_with("GET /next"), "{req}");
                ok("rel")
            }
        });
        let resp = fetch(
            &caps(&["127.0.0.1"]),
            &get(format!("http://127.0.0.1:{rel}/start")),
            &Limits::default(),
            T,
        )
        .expect("relative");
        assert_eq!(resp.body, b"rel");
    }

    #[test]
    fn redirect_loops_are_cut_off() {
        let port = serve(MAX_REDIRECTS + 1, |_| redirect("/again"));
        let err = fetch(
            &caps(&["127.0.0.1"]),
            &get(format!("http://127.0.0.1:{port}/")),
            &Limits::default(),
            T,
        )
        .expect_err("loop");
        assert!(err.contains("redirects"), "{err}");
    }

    #[test]
    fn oversized_responses_are_refused() {
        let port = serve(1, |_| ok(&"x".repeat(2048)));
        let limits = Limits {
            max_http_body_bytes: 1024,
            ..Limits::default()
        };
        assert!(
            fetch(
                &caps(&["127.0.0.1"]),
                &get(format!("http://127.0.0.1:{port}/")),
                &limits,
                T
            )
            .is_err()
        );
    }

    #[test]
    fn slow_servers_time_out() {
        let port = serve(1, |_| {
            thread::sleep(Duration::from_millis(1500));
            ok("late")
        });
        let started = Instant::now();
        let err = fetch(
            &caps(&["127.0.0.1"]),
            &get(format!("http://127.0.0.1:{port}/")),
            &Limits::default(),
            Duration::from_millis(300),
        )
        .expect_err("timeout");
        assert!(started.elapsed() < Duration::from_secs(1), "{err}");
    }

    #[test]
    fn methods_headers_and_bodies_are_validated() {
        let c = caps(&["127.0.0.1"]);
        let l = Limits::default();
        let url = "http://127.0.0.1:1/".to_owned();
        let mut r = get(url.clone());
        r.method = "CONNECT".into();
        assert!(fetch(&c, &r, &l, T).is_err());
        let mut r = get(url.clone());
        r.headers = vec![("Host".into(), "evil.org".into())];
        assert!(fetch(&c, &r, &l, T).is_err());
        let mut r = get(url.clone());
        r.headers = vec![("X-A".into(), "a\r\nB: c".into())];
        assert!(fetch(&c, &r, &l, T).is_err());
        let mut r = get(url);
        r.body = Some(vec![1]);
        assert!(fetch(&c, &r, &l, T).is_err());
    }

    #[test]
    fn post_sends_its_body() {
        let port = serve(1, |req| {
            assert!(req.starts_with("POST /p"), "{req}");
            assert!(req.ends_with("payload"), "{req}");
            ok("done")
        });
        let resp = fetch(
            &caps(&["127.0.0.1"]),
            &Request {
                method: "POST".into(),
                url: format!("http://127.0.0.1:{port}/p"),
                headers: vec![("Content-Type".into(), "text/plain".into())],
                body: Some(b"payload".to_vec()),
            },
            &Limits::default(),
            T,
        )
        .expect("post");
        assert_eq!(resp.body, b"done");
    }
}
