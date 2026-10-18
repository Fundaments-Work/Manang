use manang_kernel::{Kernel, Payload};
use manang_keystore::*;
use proptest::prelude::*;
use serde_json::{Value, json};
use std::sync::Arc;

const SECRET: &str = "sk-live-0123456789abcdefSECRET";

fn mock_keychain() -> Box<dyn KeyStore> {
    let store = keyring_core::mock::Store::new().expect("mock store");
    Box::new(KeyringStore::new(StoreKind::Keychain, store))
}

/// A store that is present but refuses everything (a locked keychain).
struct Locked;

impl KeyStore for Locked {
    fn kind(&self) -> StoreKind {
        StoreKind::Keychain
    }
    fn set(&self, _: &KeyId, _: &SecretString) -> Result<(), KeystoreError> {
        Err(KeystoreError::Backend {
            store: "keychain",
            message: "locked".into(),
        })
    }
    fn get(&self, _: &KeyId) -> Result<Option<SecretString>, KeystoreError> {
        Err(KeystoreError::Backend {
            store: "keychain",
            message: "locked".into(),
        })
    }
    fn delete(&self, _: &KeyId) -> Result<bool, KeystoreError> {
        Err(KeystoreError::Backend {
            store: "keychain",
            message: "locked".into(),
        })
    }
}

fn id() -> KeyId {
    KeyId::for_provider("deepseek").unwrap()
}

fn secret(s: &str) -> SecretString {
    SecretString::from(s.to_owned())
}

fn run(commands: &KeyCommands, command: &str, args: Value) -> Result<Value, String> {
    match commands.handle(command, Payload::Json(args)) {
        Ok(Payload::Json(v)) => Ok(v),
        Ok(other) => panic!("unexpected payload {other:?}"),
        Err(e) => Err(e.to_string()),
    }
}

fn commands(backends: Vec<Box<dyn KeyStore>>) -> KeyCommands {
    KeyCommands::new(Arc::new(ChainKeyStore::new(backends)))
}

#[test]
fn keyring_store_round_trip_on_the_mock_store() {
    let store = mock_keychain();
    assert_eq!(
        store
            .get(&id())
            .unwrap()
            .map(|k| k.expose_secret().to_owned()),
        None
    );
    store.set(&id(), &secret(SECRET)).unwrap();
    assert_eq!(store.get(&id()).unwrap().unwrap().expose_secret(), SECRET);
    assert!(store.status(&id()).unwrap().present);
    // Replace.
    store.set(&id(), &secret("second")).unwrap();
    assert_eq!(store.get(&id()).unwrap().unwrap().expose_secret(), "second");
    // Accounts are separate.
    let other = KeyId::new("deepseek", "work").unwrap();
    assert!(store.get(&other).unwrap().is_none());
    assert!(store.delete(&id()).unwrap());
    assert!(!store.delete(&id()).unwrap());
    assert!(!store.status(&id()).unwrap().present);
}

#[test]
fn session_store_is_memory_only_and_says_so() {
    let store = SessionKeyStore::new();
    assert_eq!(store.kind(), StoreKind::Session);
    assert!(!store.kind().is_saved());
    store.set(&id(), &secret(SECRET)).unwrap();
    assert_eq!(store.get(&id()).unwrap().unwrap().expose_secret(), SECRET);
    assert!(!format!("{store:?}").contains(SECRET));
    assert!(store.delete(&id()).unwrap());
    assert!(store.get(&id()).unwrap().is_none());
}

#[test]
fn chain_prefers_the_keychain_and_removes_stale_copies() {
    let chain = ChainKeyStore::new(vec![mock_keychain(), Box::new(SessionKeyStore::new())]);
    assert_eq!(chain.order(), [StoreKind::Keychain, StoreKind::Session]);
    assert_eq!(
        chain.set(&id(), &secret(SECRET)).unwrap(),
        StoreKind::Keychain
    );
    assert_eq!(chain.status(&id()).unwrap().store, StoreKind::Keychain);
    assert!(chain.delete(&id()).unwrap());
    let status = chain.status(&id()).unwrap();
    assert!(!status.present);
    assert_eq!(
        status.store,
        StoreKind::Keychain,
        "where a new key would go"
    );
}

#[test]
fn a_locked_keychain_falls_back_to_session_and_reports_it() {
    let chain = ChainKeyStore::new(vec![Box::new(Locked), Box::new(SessionKeyStore::new())]);
    assert_eq!(
        chain.set(&id(), &secret(SECRET)).unwrap(),
        StoreKind::Session
    );
    let status = chain.status(&id()).unwrap();
    assert!(status.present);
    assert_eq!(status.store, StoreKind::Session);
    assert_eq!(chain.get(&id()).unwrap().unwrap().expose_secret(), SECRET);
    // Clearing reports the locked store's failure: the key may remain there.
    assert!(chain.delete(&id()).is_err());
    assert!(
        chain.get(&id()).unwrap().is_none(),
        "the session copy is gone"
    );
}

#[test]
fn only_a_session_store_still_works() {
    let chain = ChainKeyStore::new(vec![Box::new(SessionKeyStore::new())]);
    assert_eq!(chain.set(&id(), &secret("k")).unwrap(), StoreKind::Session);
    let empty = ChainKeyStore::new(vec![]);
    assert_eq!(empty.set(&id(), &secret("k")), Err(KeystoreError::NoStore));
}

#[test]
fn commands_report_where_the_key_went_and_never_return_it() {
    let c = commands(vec![mock_keychain(), Box::new(SessionKeyStore::new())]);
    let set = run(
        &c,
        "agent.key.set",
        json!({"provider": "deepseek", "key": format!("  {SECRET}\n")}),
    )
    .unwrap();
    assert_eq!(set, json!({"store": "keychain", "saved": true}));
    let status = run(&c, "agent.key.status", json!({"provider": "deepseek"})).unwrap();
    assert_eq!(
        status,
        json!({"present": true, "store": "keychain", "saved": true})
    );
    // Trimmed on save.
    assert_eq!(
        c.store().get(&id()).unwrap().unwrap().expose_secret(),
        SECRET
    );
    let cleared = run(&c, "agent.key.clear", json!({"provider": "deepseek"})).unwrap();
    assert_eq!(cleared, json!({"cleared": true}));
    let status = run(&c, "agent.key.status", json!({"provider": "deepseek"})).unwrap();
    assert_eq!(status["present"], false);
    for text in [set.to_string(), status.to_string(), cleared.to_string()] {
        assert!(!text.contains(SECRET));
    }
}

#[test]
fn session_results_carry_the_not_saved_notice() {
    let c = commands(vec![Box::new(Locked), Box::new(SessionKeyStore::new())]);
    let set = run(
        &c,
        "agent.key.set",
        json!({"provider": "deepseek", "key": SECRET}),
    )
    .unwrap();
    assert_eq!(set["store"], "session");
    assert_eq!(set["saved"], false);
    assert!(set["notice"].as_str().unwrap().starts_with("Not saved"));
    let status = run(&c, "agent.key.status", json!({"provider": "deepseek"})).unwrap();
    assert_eq!(
        (status["present"].as_bool(), status["store"].as_str()),
        (Some(true), Some("session"))
    );
}

#[test]
fn bad_input_is_rejected_without_echoing_the_key() {
    let c = commands(vec![Box::new(SessionKeyStore::new())]);
    let cases = [
        json!({"provider": "Deep Seek", "key": SECRET}),
        json!({"provider": "", "key": SECRET}),
        json!({"provider": "deepseek", "key": "   "}),
        json!({"provider": "deepseek", "key": format!("{SECRET}\u{7}")}),
        json!({"provider": "deepseek", "key": "x".repeat(5000)}),
        json!({"provider": "deepseek", "key": SECRET, "extra": 1}),
        json!({"provider": "deepseek", "key": 5}),
        json!({"key": SECRET}),
    ];
    for case in cases {
        let error = run(&c, "agent.key.set", case).unwrap_err();
        assert!(!error.contains(SECRET), "{error}");
    }
    assert!(
        run(
            &c,
            "agent.key.status",
            json!({"provider": "x", "key": SECRET})
        )
        .is_err()
    );
    assert!(c.handle("agent.key.set", Payload::Empty).is_err());
    assert!(
        c.handle("agent.key.other", Payload::Json(json!({"provider": "x"})))
            .is_err()
    );
    assert!(
        !KeyCommands::handles("agent.key.get"),
        "there is no command that reads a key"
    );
}

#[test]
fn debug_output_never_contains_the_key() {
    let c = commands(vec![mock_keychain(), Box::new(SessionKeyStore::new())]);
    run(
        &c,
        "agent.key.set",
        json!({"provider": "deepseek", "key": SECRET}),
    )
    .unwrap();
    let key = c.store().get(&id()).unwrap().unwrap();
    for shown in [
        format!("{c:?}"),
        format!("{:?}", c.store()),
        format!("{key:?}"),
    ] {
        assert!(!shown.contains(SECRET), "{shown}");
    }
}

#[test]
fn works_through_the_kernel_as_a_plugin() {
    let manifest = manifest().expect("manifest is valid");
    assert_eq!(manifest.plugin.name, "agent");
    let kernel = Kernel::new();
    let plugin = KeystorePlugin::new(commands(vec![Box::new(SessionKeyStore::new())]));
    kernel
        .register(manifest, Arc::new(plugin))
        .expect("registers");
    let out = kernel
        .invoke(
            "agent.key.set",
            Payload::Json(json!({"provider": "anthropic", "key": SECRET})),
        )
        .unwrap();
    let Payload::Json(out) = out else {
        panic!("json expected")
    };
    assert_eq!(out["store"], "session");
    assert!(!out.to_string().contains(SECRET));
}

proptest! {
    #[test]
    fn no_key_ever_appears_in_any_result(key in "sk-[A-Za-z0-9]{8,40}") {
        let c = commands(vec![Box::new(SessionKeyStore::new())]);
        let results = [
            run(&c, "agent.key.set", json!({"provider": "deepseek", "key": key})),
            run(&c, "agent.key.status", json!({"provider": "deepseek"})),
            run(&c, "agent.key.clear", json!({"provider": "deepseek"})),
        ];
        for result in results {
            let text = match result { Ok(v) => v.to_string(), Err(e) => e };
            prop_assert!(!text.contains(&key));
        }
    }

    #[test]
    fn ids_accept_only_safe_names(s in ".{0,70}") {
        let ok = KeyId::for_provider(&s).is_ok();
        let expected = !s.is_empty() && s.len() <= 64
            && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        prop_assert_eq!(ok, expected);
    }
}

// ---- platform tests: touch the real OS stores; run by hand ---------------------

fn probe_id() -> KeyId {
    KeyId::new("manang-test-probe", "default").unwrap()
}

fn platform_round_trip(store: &dyn KeyStore) {
    store.set(&probe_id(), &secret("probe-value-1")).unwrap();
    assert_eq!(
        store.get(&probe_id()).unwrap().unwrap().expose_secret(),
        "probe-value-1"
    );
    assert!(store.delete(&probe_id()).unwrap());
    assert!(store.get(&probe_id()).unwrap().is_none());
}

#[test]
#[ignore = "touches the real OS keychain / kernel keyring"]
fn platform_detect_chain_round_trip() {
    let chain = detect();
    println!("order: {:?}", chain.order());
    let kind = chain.set(&probe_id(), &secret("probe-value-2")).unwrap();
    println!("stored in: {}", kind.as_str());
    assert!(chain.status(&probe_id()).unwrap().present);
    assert!(chain.delete(&probe_id()).unwrap());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "touches the real Linux kernel keyring"]
fn platform_kernel_keyring_round_trip() {
    let store = linux_keyutils_keyring_store::Store::new().expect("kernel store");
    platform_round_trip(&KeyringStore::new(StoreKind::Kernel, store));
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "touches the real Secret Service (may prompt to unlock)"]
fn platform_secret_service_round_trip() {
    let store = zbus_secret_service_keyring_store::Store::new().expect("secret service");
    platform_round_trip(&KeyringStore::new(StoreKind::Keychain, store));
}
