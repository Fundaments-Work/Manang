use std::sync::atomic::AtomicBool;

use crate::testing::{TestServer, sign, test_signing_key, trusting};
use crate::*;

const ARCHIVE: &[u8] = b"pretend this is a .manangplugin";

fn index_json(generated: &str, sha: &str, size: usize) -> String {
    format!(
        r#"{{"schema":1,"generated":"{generated}","plugins":[{{
          "id":"word-count","name":"Word count","author":"A","description":"d",
          "repo":"https://example.org/r","license":"MIT","tags":["writing"],"tier":"wasm",
          "readme":"plugins/word-count/README.md","latest":"0.3.1",
          "versions":[{{"version":"0.3.1","published":"2026-10-10T00:00:00Z",
            "download":"plugins/word-count/0.3.1/w.manangplugin","size":{size},"sha256":"{sha}",
            "permissions":{{"filesystem":["read:project"]}},"changelog":"c"}}]}}]}}"#
    )
}

struct Fixture {
    server: TestServer,
    key: ed25519_dalek::SigningKey,
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let f = Self {
            server: TestServer::start(),
            key: test_signing_key(9),
            dir: tempfile::tempdir().expect("tempdir"),
        };
        f.publish("2026-10-19T08:00:00Z");
        f.server
            .put("plugins/word-count/0.3.1/w.manangplugin", ARCHIVE);
        f.server.put("plugins/word-count/README.md", "# Word count");
        f.server
            .put("stats.json", r#"{"downloads":{"word-count":42}}"#);
        f
    }

    fn publish(&self, generated: &str) {
        let json = index_json(generated, &sha256_hex(ARCHIVE), ARCHIVE.len());
        self.server.put("index.json", json.as_bytes());
        self.server
            .put("index.json.sig", sign(&self.key, json.as_bytes()));
    }

    fn registry(&self) -> Registry<HttpTransport> {
        Registry::new(
            HttpTransport,
            Base::parse(self.server.url()).expect("base"),
            trusting(&self.key),
            Cache::new(Some(self.dir.path().join("cache"))),
        )
    }
}

#[test]
fn fetches_verifies_and_caches() {
    let f = Fixture::new();
    let listing = f.registry().refresh(100).expect("refresh");
    assert_eq!(listing.freshness, Freshness::Fresh);
    assert_eq!(listing.index.plugins.len(), 1);
    assert_eq!(listing.stats.downloads["word-count"], 42);
    assert_eq!(listing.fetched_at, 100);
    // A new client reads the same list from disk with no network.
    f.server.fail("index.json", 500);
    let cached = f.registry().cached().expect("cached");
    assert_eq!(cached.index, listing.index);
}

#[test]
fn a_second_refresh_is_conditional_and_a_304_serves_the_cache() {
    let f = Fixture::new();
    f.registry().refresh(100).expect("first");
    let second = f.registry().refresh(200).expect("second");
    assert_eq!(second.freshness, Freshness::Unchanged);
    assert_eq!(second.fetched_at, 200);
    assert!(
        f.server
            .requests()
            .contains(&("/index.json".to_owned(), true))
    );
    // The signature was not downloaded again.
    assert_eq!(f.server.count("/index.json.sig"), 1);
}

#[test]
fn a_bad_signature_is_refused_and_the_cache_survives() {
    let f = Fixture::new();
    f.registry().refresh(100).expect("good");
    f.publish("2026-10-20T08:00:00Z");
    f.server.put(
        "index.json.sig",
        sign(&test_signing_key(77), b"something else"),
    );
    let err = f.registry().refresh(200).expect_err("bad signature");
    assert!(matches!(err, RegistryError::Signature));
    assert!(err.to_string().contains("nothing was installed"));
    assert!(f.registry().cached().is_some());
}

#[test]
fn a_tampered_index_does_not_verify() {
    let f = Fixture::new();
    let json = index_json("2026-10-19T08:00:00Z", &sha256_hex(ARCHIVE), ARCHIVE.len());
    let sig = sign(&f.key, json.as_bytes());
    f.server
        .put("index.json", json.replace("Word count", "Evil count"));
    f.server.put("index.json.sig", sig);
    assert!(matches!(
        f.registry().refresh(1),
        Err(RegistryError::Signature)
    ));
}

#[test]
fn a_tampered_cache_file_is_ignored() {
    let f = Fixture::new();
    f.registry().refresh(100).expect("good");
    let path = f.dir.path().join("cache/index.json");
    let text = std::fs::read_to_string(&path).expect("read");
    std::fs::write(&path, text.replace("Word count", "Evil count")).expect("write");
    assert!(f.registry().cached().is_none());
}

#[test]
fn schema_two_is_signed_but_refused_clearly() {
    let f = Fixture::new();
    let json = index_json("2026-10-19T08:00:00Z", &sha256_hex(ARCHIVE), 1)
        .replace("\"schema\":1", "\"schema\":2");
    f.server.put("index.json", json.as_bytes());
    f.server
        .put("index.json.sig", sign(&f.key, json.as_bytes()));
    let err = f.registry().refresh(1).expect_err("schema 2");
    assert!(matches!(err, RegistryError::UnsupportedSchema { found: 2 }));
}

#[test]
fn offline_with_a_cache_serves_it_and_says_so() {
    let f = Fixture::new();
    f.registry().refresh(100).expect("good");
    f.server.fail("index.json", 503);
    let listing = f.registry().refresh(300).expect("offline");
    assert!(matches!(listing.freshness, Freshness::Offline { .. }));
    assert_eq!(listing.fetched_at, 100);
    // Truly unreachable: nothing listens any more.
    let dead = Registry::new(
        HttpTransport,
        Base::parse("http://127.0.0.1:9").expect("base"),
        trusting(&f.key),
        Cache::new(Some(f.dir.path().join("cache"))),
    );
    // Another registry's cache is not served.
    assert!(matches!(
        dead.refresh(1),
        Err(RegistryError::Network { .. })
    ));
}

#[test]
fn offline_without_a_cache_is_an_unreachable_error() {
    let f = Fixture::new();
    f.server.fail("index.json", 500);
    let err = f.registry().refresh(1).expect_err("no cache");
    assert!(err.is_unreachable());
}

#[test]
fn an_older_list_than_the_cached_one_is_refused() {
    let f = Fixture::new();
    f.registry().refresh(100).expect("good");
    f.publish("2026-01-01T00:00:00Z");
    assert!(matches!(
        f.registry().refresh(200),
        Err(RegistryError::Stale)
    ));
}

#[test]
fn only_the_placeholder_key_means_not_configured() {
    let f = Fixture::new();
    let r = Registry::new(
        HttpTransport,
        Base::parse(f.server.url()).expect("base"),
        TrustedKeys::embedded(),
        Cache::default(),
    );
    assert!(matches!(r.refresh(1), Err(RegistryError::NotConfigured)));
    assert_eq!(f.server.requests().len(), 0);
}

fn version(f: &Fixture) -> VersionEntry {
    let listing = f.registry().refresh(1).expect("refresh");
    listing.index.plugins[0].versions[0].clone()
}

#[test]
fn downloads_and_verifies_size_and_hash() {
    let f = Fixture::new();
    let v = version(&f);
    let dest = f.dir.path().join("out.manangplugin");
    let mut last = (0, 0);
    f.registry()
        .download(&v, &dest, &AtomicBool::new(false), &mut |a, b| {
            last = (a, b)
        })
        .expect("download");
    assert_eq!(std::fs::read(&dest).expect("read"), ARCHIVE);
    assert_eq!(last, (ARCHIVE.len() as u64, ARCHIVE.len() as u64));
}

#[test]
fn a_wrong_hash_or_size_installs_nothing() {
    let f = Fixture::new();
    let dest = f.dir.path().join("out.manangplugin");
    let mut v = version(&f);
    f.server.put(
        "plugins/word-count/0.3.1/w.manangplugin",
        b"other bytes, same?".to_vec(),
    );
    v.size = 18;
    let err = f
        .registry()
        .download(&v, &dest, &AtomicBool::new(false), &mut |_, _| {})
        .expect_err("hash");
    assert!(matches!(err, RegistryError::HashMismatch));
    assert!(!dest.exists());

    let v = version(&f);
    let err = f
        .registry()
        .download(&v, &dest, &AtomicBool::new(false), &mut |_, _| {})
        .expect_err("size");
    assert!(matches!(err, RegistryError::SizeMismatch));
    assert!(!dest.exists());
}

#[test]
fn a_cancelled_download_leaves_no_file() {
    let f = Fixture::new();
    let v = version(&f);
    let dest = f.dir.path().join("out.manangplugin");
    let err = f
        .registry()
        .download(&v, &dest, &AtomicBool::new(true), &mut |_, _| {})
        .expect_err("cancel");
    assert!(matches!(err, RegistryError::Cancelled));
    assert!(!dest.exists());
}

#[test]
fn readmes_are_fetched_with_a_limit() {
    let f = Fixture::new();
    let r = f.registry();
    assert_eq!(
        r.fetch_text("plugins/word-count/README.md", 1024)
            .expect("text"),
        "# Word count"
    );
    assert!(matches!(
        r.fetch_text("plugins/word-count/README.md", 3),
        Err(RegistryError::TooLarge { .. })
    ));
}
