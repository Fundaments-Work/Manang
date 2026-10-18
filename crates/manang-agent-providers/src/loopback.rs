//! Offline mode: only loopback destinations.
//!
//! The check resolves the host and inspects every address; a string prefix
//! such as `http://localhost` proves nothing (`localhost.evil.test` or a
//! rebinding DNS answer would pass it).

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::{IpAddr, SocketAddr};
use url::{Host, Url};

fn parse(url: &str) -> Result<Url, String> {
    let url = Url::parse(url).map_err(|_| "invalid URL".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http(s) URLs are allowed".to_owned());
    }
    Ok(url)
}

fn blocked(what: &str) -> String {
    format!("offline mode allows only loopback addresses, not {what}")
}

/// Checks an IP-literal host without touching DNS; hostnames pass here (they
/// are checked by [`ensure_loopback`] and by the resolver).
pub fn ensure_loopback_literal(url: &str) -> Result<(), String> {
    let url = parse(url)?;
    match url.host() {
        Some(Host::Ipv4(ip)) if !ip.is_loopback() => Err(blocked("this address")),
        Some(Host::Ipv6(ip)) if !ip.is_loopback() => Err(blocked("this address")),
        Some(_) => Ok(()),
        None => Err("URL has no host".to_owned()),
    }
}

/// Resolves the host of `url` and requires every address to be loopback.
pub async fn ensure_loopback(url: &str) -> Result<(), String> {
    let parsed = parse(url)?;
    ensure_loopback_literal(url)?;
    let Some(Host::Domain(host)) = parsed.host() else {
        return Ok(());
    };
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| "could not resolve the host".to_owned())?
        .collect();
    check_addresses(&addrs)
}

fn check_addresses(addrs: &[SocketAddr]) -> Result<(), String> {
    if addrs.is_empty() {
        return Err("the host resolved to no addresses".to_owned());
    }
    if addrs.iter().all(|a| is_loopback(a.ip())) {
        Ok(())
    } else {
        Err(blocked("the address the host resolves to"))
    }
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    }
}

/// A DNS resolver for reqwest that refuses non-loopback answers, so the
/// check also holds at connect time (no resolve-then-rebind gap).
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoopbackResolver;

impl Resolve for LoopbackResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                .collect();
            check_addresses(&addrs).map_err(Box::<dyn std::error::Error + Send + Sync>::from)?;
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn loopback_hosts_pass() {
        for url in [
            "http://127.0.0.1:11434/v1",
            "http://[::1]:8080",
            "http://localhost:11434/v1",
        ] {
            assert_eq!(ensure_loopback(url).await, Ok(()), "{url}");
        }
    }

    #[tokio::test]
    async fn non_loopback_literals_and_prefix_tricks_fail() {
        for url in [
            "http://192.168.1.10:11434",
            "http://10.0.0.1",
            "http://8.8.8.8",
            "http://[2001:db8::1]/",
            // A string-prefix check would accept this; resolution does not.
            "http://127.0.0.1.nip.invalid/",
            "ftp://127.0.0.1/",
            "not a url",
        ] {
            assert!(ensure_loopback(url).await.is_err(), "{url}");
        }
    }

    #[test]
    fn mixed_answers_are_rejected() {
        let local: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let remote: SocketAddr = "203.0.113.5:1".parse().unwrap();
        assert!(check_addresses(&[local]).is_ok());
        assert!(check_addresses(&[local, remote]).is_err());
        assert!(check_addresses(&[]).is_err());
    }
}
