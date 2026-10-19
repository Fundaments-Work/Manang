//! The plugin marketplace end to end through the kernel's `plugins.*`
//! commands: a local server plays the registry, signs its index with a test
//! key, and serves real plugin archives.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{Kernel, Payload};
use manang_plugins::{
    ManagerConfig, ManagerEvent, MarketplaceConfig, PluginManager, PluginStatus, manifest,
    plugin_with,
};
use manang_registry::testing::{TestServer, sign, test_signing_key, trusting};
use manang_registry::{HttpTransport, sha256_hex};
use manang_wasm_host::Limits;
use parking_lot::Mutex;
use serde_json::{Value, json};

mod common;
use common::{archive, component, manifest_with};

struct Spec {
    id: &'static str,
    version: &'static str,
    hosts: Vec<&'static str>,
    /// What the index claims; `None` means what the manifest really asks.
    listed_hosts: Option<Vec<&'static str>>,
    answer: char,
}

fn spec(id: &'static str, version: &'static str, hosts: &[&'static str]) -> Spec {
    Spec {
        id,
        version,
        hosts: hosts.to_vec(),
        listed_hosts: None,
        answer: '1',
    }
}

struct Market {
    tmp: tempfile::TempDir,
    server: TestServer,
    key: ed25519_dalek::SigningKey,
    kernel: Arc<Kernel>,
    manager: Arc<PluginManager>,
    events: Arc<Mutex<Vec<ManagerEvent>>>,
}

impl Market {
    fn new() -> Self {
        Self::with(|_| {})
    }

    fn with(tweak: impl FnOnce(&mut MarketplaceConfig)) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let server = TestServer::start();
        let key = test_signing_key(5);
        let events: Arc<Mutex<Vec<ManagerEvent>>> = Arc::default();
        let mut config = ManagerConfig::new(tmp.path().join("plugins"));
        config.registry_path = Some(tmp.path().join("config").join("plugins.toml"));
        config.trash_dir = Some(tmp.path().join("trash"));
        config.limits = Limits {
            deadline: Duration::from_secs(20),
            ..Limits::default()
        };
        let sink = Arc::clone(&events);
        config.listener = Some(Arc::new(move |e| sink.lock().push(e.clone())));
        let kernel = Arc::new(Kernel::new());
        let manager = Arc::new(PluginManager::new(Arc::clone(&kernel), config).expect("manager"));
        let mut market = MarketplaceConfig {
            cache_dir: Some(tmp.path().join("registry-cache")),
            registry_url: server.url().to_owned(),
            keys: trusting(&key),
            transport: Arc::new(HttpTransport),
            app_version: "0.1.0".to_owned(),
        };
        tweak(&mut market);
        kernel
            .register(
                manifest().expect("manifest"),
                Arc::new(plugin_with(&manager, market)),
            )
            .expect("register");
        Self {
            tmp,
            server,
            key,
            kernel,
            manager,
            events,
        }
    }

    fn run(&self, command: &str, args: Value) -> Result<Value, String> {
        match self.kernel.invoke(command, Payload::Json(args)) {
            Ok(Payload::Json(v)) => Ok(v),
            Ok(_) => Ok(Value::Null),
            Err(e) => Err(e.to_string()),
        }
    }

    fn ok(&self, command: &str, args: Value) -> Value {
        self.run(command, args)
            .unwrap_or_else(|e| panic!("{command} failed: {e}"))
    }

    /// Serve `specs` as the registry's catalogue, with real archives.
    fn publish(&self, generated: &str, specs: &[Spec]) {
        let mut plugins = Vec::new();
        for s in specs {
            let manifest = manifest_with(s.id, s.version, &s.hosts);
            let wasm = component(s.answer, 1);
            let path = self
                .tmp
                .path()
                .join(format!("{}-{}.manangplugin", s.id, s.version));
            archive(
                &path,
                &[
                    ("manifest.toml", manifest.as_bytes()),
                    ("plugin.wasm", &wasm),
                ],
            );
            let bytes = std::fs::read(&path).expect("archive");
            let rel = format!(
                "plugins/{}/{}/{}-{}.manangplugin",
                s.id, s.version, s.id, s.version
            );
            self.server.put(&rel, bytes.clone());
            let listed = s.listed_hosts.as_ref().unwrap_or(&s.hosts);
            plugins.push((s, rel, sha256_hex(&bytes), bytes.len(), listed.clone()));
        }
        // One entry per id; its versions are the specs sharing the id, the
        // newest listed last.
        let mut entries: Vec<Value> = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for (s, ..) in &plugins {
            if seen.contains(&s.id) {
                continue;
            }
            seen.push(s.id);
            let versions: Vec<Value> = plugins
                .iter()
                .filter(|(o, ..)| o.id == s.id)
                .map(|(o, rel, sha, size, listed)| {
                    json!({
                        "version": o.version, "published": "2026-10-10T12:00:00Z",
                        "download": rel, "size": size, "sha256": sha,
                        "permissions": { "network": listed },
                        "changelog": format!("Release {}", o.version),
                    })
                })
                .collect();
            let latest = plugins
                .iter()
                .filter(|(o, ..)| o.id == s.id)
                .map(|(o, ..)| o.version)
                .next_back()
                .unwrap_or(s.version);
            entries.push(json!({
                "id": s.id, "name": s.id.to_uppercase(), "author": "Test Author",
                "description": format!("{} does things", s.id),
                "repo": "https://example.org/repo", "license": "MIT",
                "tags": ["writing"], "tier": "wasm", "latest": latest,
                "readme": format!("plugins/{}/README.md", s.id),
                "versions": versions,
            }));
            self.server.put(
                &format!("plugins/{}/README.md", s.id),
                format!("# {}", s.id),
            );
        }
        let index = json!({ "schema": 1, "generated": generated, "plugins": entries });
        self.put_index(&serde_json::to_vec(&index).expect("json"));
    }

    fn put_index(&self, bytes: &[u8]) {
        self.server.put("index.json", bytes.to_vec());
        self.server.put("index.json.sig", sign(&self.key, bytes));
    }

    fn installed_dirs(&self) -> Vec<String> {
        let dir: PathBuf = self.tmp.path().join("plugins");
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("read")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn plugins_toml(&self) -> String {
        std::fs::read_to_string(Path::new(self.tmp.path()).join("config/plugins.toml"))
            .unwrap_or_default()
    }
}

fn names(v: &Value) -> Vec<String> {
    v["plugins"]
        .as_array()
        .expect("plugins")
        .iter()
        .map(|p| p["id"].as_str().expect("id").to_owned())
        .collect()
}

#[test]
fn browse_lists_the_verified_index_with_install_state() {
    let m = Market::new();
    m.publish(
        "2026-10-19T08:00:00Z",
        &[
            spec("alpha", "0.1.0", &[]),
            spec("beta", "0.2.0", &["api.example.org"]),
        ],
    );
    let out = m.ok("plugins.browse", json!({}));
    assert_eq!(names(&out), ["alpha", "beta"]);
    assert_eq!(out["freshness"]["state"], "fresh");
    assert_eq!(out["plugins"][0]["installed"], Value::Null);
    let found = m.ok("plugins.browse", json!({ "text": "BETA" }));
    assert_eq!(names(&found), ["beta"]);
    let detail = m.ok("plugins.detail", json!({ "id": "beta" }));
    assert_eq!(detail["readme"], "# beta");
    assert_eq!(detail["permissions"]["network"][0], "api.example.org");
    assert!(m.run("plugins.detail", json!({ "id": "nope" })).is_err());
}

#[test]
fn install_needs_consent_then_installs_approves_and_records_the_source() {
    let m = Market::new();
    m.publish(
        "2026-10-19T08:00:00Z",
        &[spec("alpha", "0.1.0", &["api.example.org"])],
    );
    let err = m
        .run("plugins.install-from-registry", json!({ "id": "alpha" }))
        .expect_err("no consent");
    assert!(err.contains("[consent_required]"), "{err}");
    let err = m
        .run(
            "plugins.install-from-registry",
            json!({ "id": "alpha", "consent": false }),
        )
        .expect_err("consent false");
    assert!(err.contains("[consent_required]"), "{err}");
    assert!(m.installed_dirs().is_empty());
    assert_eq!(
        m.server
            .count("/plugins/alpha/0.1.0/alpha-0.1.0.manangplugin"),
        0
    );

    let state = m.ok(
        "plugins.install-from-registry",
        json!({ "id": "alpha", "consent": true }),
    );
    assert_eq!(state["name"], "alpha");
    assert_eq!(state["status"], "loaded");
    assert_eq!(state["origin"]["id"], "alpha");
    assert_eq!(state["origin"]["version"], "0.1.0");
    assert_eq!(state["granted"]["network"][0], "api.example.org");
    assert_eq!(
        m.installed_dirs(),
        ["alpha"],
        "no download scratch left behind"
    );
    let toml = m.plugins_toml();
    assert!(toml.contains("[plugins.alpha.origin]"), "{toml}");
    assert!(toml.contains("id = \"alpha\""), "{toml}");
    let phases: Vec<&str> = m
        .events
        .lock()
        .iter()
        .filter_map(|e| match e {
            ManagerEvent::InstallProgress { phase, .. } => Some(*phase),
            _ => None,
        })
        .collect();
    assert_eq!(phases.first(), Some(&"downloading"));
    assert_eq!(phases.last(), Some(&"done"));
    // Browse now shows it installed.
    let out = m.ok("plugins.browse", json!({}));
    assert_eq!(out["plugins"][0]["installed"]["version"], "0.1.0");
}

#[test]
fn a_bad_signature_blocks_browse_and_install() {
    let m = Market::new();
    m.publish("2026-10-19T08:00:00Z", &[spec("alpha", "0.1.0", &[])]);
    m.server
        .put("index.json.sig", sign(&test_signing_key(99), b"other"));
    for (cmd, args) in [
        ("plugins.browse", json!({})),
        (
            "plugins.install-from-registry",
            json!({ "id": "alpha", "consent": true }),
        ),
    ] {
        let err = m.run(cmd, args).expect_err("signature");
        assert!(err.contains("[registry_signature]"), "{err}");
        assert!(err.contains("could not be verified"), "{err}");
    }
    assert!(m.installed_dirs().is_empty());
}

#[test]
fn a_tampered_index_is_refused() {
    let m = Market::new();
    m.publish("2026-10-19T08:00:00Z", &[spec("alpha", "0.1.0", &[])]);
    let good = {
        let reg = m.server.url().to_owned();
        assert!(reg.starts_with("http://127.0.0.1"));
        m.ok("plugins.browse", json!({}));
        std::fs::read(m.tmp.path().join("registry-cache/index.json")).expect("cache")
    };
    // Same signature, different bytes.
    let sig = sign(&m.key, &good);
    let tampered = String::from_utf8(good)
        .expect("utf8")
        .replace("ALPHA", "EVIL");
    m.server.put("index.json", tampered.into_bytes());
    m.server.put("index.json.sig", sig);
    let err = m
        .run("plugins.browse", json!({ "refresh": true }))
        .expect_err("tampered");
    assert!(err.contains("[registry_signature]"), "{err}");
}

#[test]
fn schema_two_asks_the_person_to_update() {
    let m = Market::new();
    m.put_index(br#"{"schema":2,"generated":"2026-10-19T08:00:00Z","plugins":[]}"#);
    let err = m.run("plugins.browse", json!({})).expect_err("schema 2");
    assert!(err.contains("[registry_schema]"), "{err}");
    assert!(err.contains("update Manang"), "{err}");
}

#[test]
fn a_hash_or_size_mismatch_installs_nothing() {
    let m = Market::new();
    m.publish("2026-10-19T08:00:00Z", &[spec("alpha", "0.1.0", &[])]);
    let rel = "plugins/alpha/0.1.0/alpha-0.1.0.manangplugin";
    let genuine = std::fs::read(m.tmp.path().join("alpha-0.1.0.manangplugin")).expect("archive");
    // Same length, one byte changed: the hash differs.
    let mut changed = genuine.clone();
    if let Some(b) = changed.last_mut() {
        *b ^= 0xff;
    }
    m.server.put(rel, changed);
    let err = m
        .run(
            "plugins.install-from-registry",
            json!({ "id": "alpha", "consent": true }),
        )
        .expect_err("hash");
    assert!(err.contains("[registry_hash_mismatch]"), "{err}");
    // A different length: the size differs.
    let mut longer = genuine;
    longer.push(0);
    m.server.put(rel, longer);
    let err = m
        .run(
            "plugins.install-from-registry",
            json!({ "id": "alpha", "consent": true }),
        )
        .expect_err("size");
    assert!(err.contains("[registry_size_mismatch]"), "{err}");
    assert!(m.installed_dirs().is_empty(), "{:?}", m.installed_dirs());
}

#[test]
fn a_manifest_that_disagrees_with_the_index_is_refused() {
    let m = Market::new();
    let mut s = spec("alpha", "0.1.0", &["tracker.example.net"]);
    s.listed_hosts = Some(vec![]);
    m.publish("2026-10-19T08:00:00Z", &[s]);
    let err = m
        .run(
            "plugins.install-from-registry",
            json!({ "id": "alpha", "consent": true }),
        )
        .expect_err("mismatch");
    assert!(err.contains("[registry_mismatch]"), "{err}");
    assert!(err.contains("nothing was installed"), "{err}");
    assert!(m.installed_dirs().is_empty());
    assert!(m.manager.state_of("alpha").is_err());
}

#[test]
fn offline_serves_the_cache_with_its_date_and_304_is_handled() {
    let m = Market::new();
    m.publish("2026-10-19T08:00:00Z", &[spec("alpha", "0.1.0", &[])]);
    let first = m.ok("plugins.browse", json!({ "refresh": true }));
    assert_eq!(first["freshness"]["state"], "fresh");
    let when = first["freshness"]["fetched_at"].as_u64().expect("date");
    // Unchanged: a conditional request, answered 304, still a fresh answer.
    let second = m.ok("plugins.browse", json!({ "refresh": true }));
    assert_eq!(second["freshness"]["state"], "unchanged");
    assert!(
        m.server
            .requests()
            .iter()
            .any(|(p, c)| p == "/index.json" && *c)
    );
    // Down: the cached list, with the date of the last contact.
    m.server.fail("index.json", 503);
    let down = m.ok("plugins.browse", json!({ "refresh": true }));
    assert_eq!(down["freshness"]["state"], "offline");
    assert!(down["freshness"]["fetched_at"].as_u64().expect("date") >= when);
    assert_eq!(names(&down), ["alpha"]);
    // Without refresh the cache answers with no network at all.
    let before = m.server.requests().len();
    let cached = m.ok("plugins.browse", json!({}));
    assert_eq!(cached["freshness"]["state"], "cached");
    assert_eq!(m.server.requests().len(), before);
}

#[test]
fn an_unreachable_registry_with_no_cache_says_so() {
    let m = Market::new();
    m.server.fail("index.json", 500);
    let err = m.run("plugins.browse", json!({})).expect_err("down");
    assert!(err.contains("[registry_unreachable]"), "{err}");
}

#[test]
fn updates_are_detected_never_installed_and_ask_again_when_permissions_grow() {
    let m = Market::new();
    m.publish(
        "2026-10-19T08:00:00Z",
        &[spec("alpha", "0.1.0", &["a.example.org"])],
    );
    m.ok(
        "plugins.install-from-registry",
        json!({ "id": "alpha", "consent": true }),
    );
    let none = m.ok("plugins.check-updates", json!({}));
    assert_eq!(none["updates"].as_array().expect("updates").len(), 0);

    // 0.2.0 asks for one more host.
    m.publish(
        "2026-10-20T08:00:00Z",
        &[
            spec("alpha", "0.1.0", &["a.example.org"]),
            spec("alpha", "0.2.0", &["a.example.org", "b.example.org"]),
        ],
    );
    let found = m.ok("plugins.check-updates", json!({}));
    let updates = found["updates"].as_array().expect("updates");
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["latest"], "0.2.0");
    assert_eq!(updates[0]["installed"], "0.1.0");
    assert_eq!(updates[0]["permissions_grew"], true);
    assert_eq!(updates[0]["new_permissions"]["network"][0], "b.example.org");
    // Checking installed nothing.
    assert_eq!(m.manager.state_of("alpha").expect("state").version, "0.1.0");
    assert!(
        m.events
            .lock()
            .iter()
            .any(|e| matches!(e, ManagerEvent::UpdatesAvailable { count: 1 }))
    );
    // The status counts it without the network.
    assert_eq!(m.ok("plugins.registry-status", json!({}))["updates"], 1);

    // Updating takes its own consent and lands approved.
    let err = m
        .run("plugins.install-from-registry", json!({ "id": "alpha" }))
        .expect_err("consent");
    assert!(err.contains("[consent_required]"), "{err}");
    let state = m.ok(
        "plugins.install-from-registry",
        json!({ "id": "alpha", "consent": true }),
    );
    assert_eq!(state["version"], "0.2.0");
    assert_eq!(state["status"], "loaded");
    assert_eq!(state["origin"]["version"], "0.2.0");
    assert_eq!(
        state["granted"]["network"].as_array().expect("net").len(),
        2
    );
    assert_eq!(
        m.ok("plugins.check-updates", json!({}))["updates"]
            .as_array()
            .expect("updates")
            .len(),
        0
    );
    assert_eq!(
        m.manager.state_of("alpha").expect("state").status,
        PluginStatus::Loaded
    );
}

#[test]
fn an_update_within_the_approved_access_is_flagged_as_not_growing() {
    let m = Market::new();
    m.publish(
        "2026-10-19T08:00:00Z",
        &[spec("alpha", "0.1.0", &["a.example.org"])],
    );
    m.ok(
        "plugins.install-from-registry",
        json!({ "id": "alpha", "consent": true }),
    );
    m.publish(
        "2026-10-20T08:00:00Z",
        &[
            spec("alpha", "0.1.0", &["a.example.org"]),
            spec("alpha", "0.1.1", &["a.example.org"]),
        ],
    );
    let found = m.ok("plugins.check-updates", json!({}));
    assert_eq!(found["updates"][0]["permissions_grew"], false);
}

#[test]
fn a_background_check_within_a_day_does_not_touch_the_network() {
    let m = Market::new();
    m.publish("2026-10-19T08:00:00Z", &[spec("alpha", "0.1.0", &[])]);
    m.ok("plugins.check-updates", json!({}));
    let before = m.server.requests().len();
    let again = m.ok("plugins.check-updates", json!({ "background": true }));
    assert_eq!(again["freshness"]["state"], "cached");
    assert_eq!(m.server.requests().len(), before);
}

#[test]
fn browse_can_be_switched_off_and_the_address_overridden() {
    let m = Market::new();
    m.publish("2026-10-19T08:00:00Z", &[spec("alpha", "0.1.0", &[])]);
    m.ok(
        "plugins.configure",
        json!({ "values": { "plugins.browse": false } }),
    );
    let before = m.server.requests().len();
    let err = m.run("plugins.browse", json!({})).expect_err("off");
    assert!(err.contains("[registry_off]"), "{err}");
    let err = m
        .run(
            "plugins.install-from-registry",
            json!({ "id": "alpha", "consent": true }),
        )
        .expect_err("off");
    assert!(err.contains("[registry_off]"), "{err}");
    let checked = m.ok("plugins.check-updates", json!({}));
    assert_eq!(checked["enabled"], false);
    assert_eq!(m.ok("plugins.registry-status", json!({}))["enabled"], false);
    assert_eq!(m.server.requests().len(), before, "no contact while off");

    m.ok(
        "plugins.configure",
        json!({ "values": { "plugins.browse": true } }),
    );
    assert!(m.run("plugins.browse", json!({})).is_ok());
    let err = m
        .run(
            "plugins.configure",
            json!({ "values": { "plugins.registry-url": "http://plugins.example.org" } }),
        )
        .expect_err("insecure");
    assert!(err.contains("https://"), "{err}");
    // A good override is used.
    let other = TestServer::start();
    m.ok(
        "plugins.configure",
        json!({ "values": { "plugins.registry-url": other.url() } }),
    );
    assert!(
        m.run("plugins.browse", json!({})).is_err(),
        "other server serves nothing"
    );
    assert!(other.requests().iter().any(|(p, _)| p == "/index.json"));
}

#[test]
fn the_placeholder_build_has_no_registry() {
    let m = Market::with(|c| {
        c.registry_url = manang_registry::PLACEHOLDER_URL.to_owned();
        c.keys = manang_registry::TrustedKeys::embedded();
    });
    let status = m.ok("plugins.registry-status", json!({}));
    assert_eq!(status["configured"], false);
    let err = m
        .run("plugins.browse", json!({}))
        .expect_err("not configured");
    assert!(err.contains("[registry_not_configured]"), "{err}");
}

#[test]
fn a_withdrawn_version_is_not_offered_to_install_or_update() {
    let m = Market::new();
    m.publish(
        "2026-10-19T08:00:00Z",
        &[spec("alpha", "0.1.0", &[]), spec("alpha", "0.2.0", &[])],
    );
    m.ok(
        "plugins.install-from-registry",
        json!({ "id": "alpha", "version": "0.1.0", "consent": true }),
    );
    // The registry withdraws 0.2.0 (the signed index gains a `yanked` list).
    let cached = std::fs::read(m.tmp.path().join("registry-cache/index.json")).expect("cache");
    let mut index: Value = serde_json::from_slice(&cached).expect("json");
    index["generated"] = json!("2026-10-20T08:00:00Z");
    index["plugins"][0]["yanked"] = json!(["0.2.0"]);
    m.put_index(&serde_json::to_vec(&index).expect("json"));
    let found = m.ok("plugins.check-updates", json!({}));
    assert_eq!(found["updates"].as_array().expect("updates").len(), 0);
    let err = m
        .run(
            "plugins.install-from-registry",
            json!({ "id": "alpha", "version": "0.2.0", "consent": true }),
        )
        .expect_err("withdrawn");
    assert!(err.contains("[registry_withdrawn]"), "{err}");
    let detail = m.ok("plugins.detail", json!({ "id": "alpha" }));
    assert_eq!(detail["versions"][1]["yanked"], true);
}
