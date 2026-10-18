use manang_settings::{Registry, Scope, SettingDef, SettingType, layers};
use proptest::prelude::*;
use serde_json::{Value, json};

#[test]
fn core_registry_builds_and_defaults_validate() {
    let reg = Registry::with_core();
    assert_eq!(reg.len(), 19);
    for def in reg.iter() {
        assert_eq!(
            def.validate(&def.default).as_ref(),
            Ok(&def.default),
            "{}",
            def.id
        );
    }
}

#[test]
fn core_categories_declare_their_contexts() {
    let reg = Registry::with_core();
    let contexts = |id: &str| reg.get(id).map(|d| d.contexts.clone()).unwrap_or_default();
    assert_eq!(contexts("appearance.theme"), ["app"]);
    assert_eq!(contexts("editor.font-size"), ["document", "diagram"]);
    assert_eq!(contexts("preview.auto-compile"), ["document", "pdf"]);
    assert_eq!(contexts("compile.engine"), ["document"]);
    assert_eq!(contexts("files.autosave"), ["app", "project", "document"]);
    // Every setting of a category shares its category's contexts.
    for def in reg.iter() {
        assert_eq!(
            def.contexts,
            manang_settings::defaults::category_contexts(&def.category),
            "{}",
            def.id
        );
    }
}

#[test]
fn registration_fills_and_validates_contexts() {
    let mut reg = Registry::new();
    let plain = SettingDef::new(
        "demo.flag",
        SettingType::Bool,
        json!(true),
        Scope::User,
        false,
        "A flag.",
    );
    reg.register(plain.clone(), "demo").expect("register");
    assert_eq!(
        reg.get("demo.flag").map(|d| d.contexts.clone()),
        Some(vec!["app".to_owned()])
    );

    let scoped = SettingDef::new(
        "other.flag",
        SettingType::Bool,
        json!(true),
        Scope::User,
        false,
        "A flag.",
    )
    .with_contexts(&["document", "engine:typst"]);
    reg.register(scoped, "other").expect("register");
    assert_eq!(
        reg.get("other.flag").map(|d| d.contexts.clone()),
        Some(vec!["document".to_owned(), "engine:typst".to_owned()])
    );

    for bad in [&["editor"][..], &["engine:"], &["document", "document"]] {
        let def = SettingDef::new(
            "bad.flag",
            SettingType::Bool,
            json!(true),
            Scope::User,
            false,
            "A flag.",
        )
        .with_contexts(bad);
        let err = reg.register(def, "bad").expect_err("invalid contexts");
        assert!(err.to_string().contains("settings context"), "{err}");
    }
    assert!(reg.get("bad.flag").is_none());

    // Contexts arrive over `settings.register` as JSON.
    let def: SettingDef = serde_json::from_value(json!({
        "id": "json.flag", "type": "bool", "default": false,
        "description": "A flag.", "contexts": ["figure"],
    }))
    .expect("parse");
    reg.register(def, "json").expect("register");
    assert_eq!(
        reg.get("json.flag").map(|d| d.contexts.clone()),
        Some(vec!["figure".to_owned()])
    );
}

#[test]
fn schema_snapshot() {
    insta::assert_json_snapshot!(Registry::with_core().schema());
}

#[test]
fn validation_error_snapshot() {
    let reg = Registry::with_core();
    let cases: Vec<(&str, Value)> = vec![
        ("editor.font-size", json!(99)),
        ("editor.font-size", json!(2)),
        ("editor.font-size", json!(12.5)),
        ("editor.font-size", json!("big")),
        ("editor.line-height", json!(0.2)),
        ("appearance.theme", json!("sepia")),
        ("appearance.theme", json!(3)),
        ("editor.word-wrap", json!("yes")),
        ("preview.debounce-ms", json!(null)),
        ("appearance.ui-scale", json!([1])),
    ];
    let out: Vec<String> = cases
        .into_iter()
        .map(|(id, v)| match reg.get(id).map(|d| d.validate(&v)) {
            Some(Err(e)) => e.to_string(),
            other => format!("unexpected: {other:?}"),
        })
        .collect();
    insta::assert_debug_snapshot!(out);
}

#[test]
fn list_and_path_validation() {
    let list = SettingType::List {
        item: Box::new(SettingType::Path),
    };
    assert!(list.validate("a.b", &json!(["x", "y"])).is_ok());
    let err = list.validate("a.b", &json!(["x", ""])).expect_err("empty");
    assert_eq!(err.to_string(), "`a.b[1]` must not be empty");
    let err = list.validate("a.b", &json!(["x", 3])).expect_err("type");
    assert_eq!(
        err.to_string(),
        "`a.b[1]` must be a path string, found a number"
    );
}

#[test]
fn registration_rules() {
    let mut reg = Registry::with_core();
    let mine = |id: &str| {
        SettingDef::new(
            id,
            SettingType::Int {
                min: Some(1),
                max: Some(5),
            },
            json!(3),
            Scope::Project,
            true,
            "A test setting.",
        )
    };
    assert!(reg.register(mine("my-plugin.width"), "my-plugin").is_ok());
    // Idempotent for the same owner and definition.
    assert!(reg.register(mine("my-plugin.width"), "my-plugin").is_ok());
    // The same owner may change its definition (plugin hot reload).
    let changed = mine("my-plugin.width").with_title("Width");
    assert_eq!(
        reg.register_all(vec![changed.clone()], "my-plugin").ok(),
        Some(1)
    );
    assert_eq!(
        reg.get("my-plugin.width").and_then(|d| d.title.as_deref()),
        Some("Width")
    );
    assert_eq!(reg.register_all(vec![changed], "my-plugin").ok(), Some(0));
    // Different owner is refused.
    assert!(reg.register(mine("my-plugin.width"), "other").is_err());
    // Core ids are protected.
    assert!(reg.register(mine("editor.tab-size"), "my-plugin").is_err());
    // Overlap with an existing id is refused.
    assert!(
        reg.register(mine("my-plugin.width.x"), "my-plugin")
            .is_err()
    );
    // Bad ids and bad defaults are refused and nothing partial is added.
    let before = reg.len();
    let mut bad_default = mine("my-plugin.height");
    bad_default.default = json!(50);
    let batch = vec![mine("my-plugin.ok"), bad_default];
    assert!(reg.register_all(batch, "my-plugin").is_err());
    assert_eq!(reg.len(), before);
    assert!(reg.register(mine("Bad.Id"), "my-plugin").is_err());
    assert!(reg.register(mine("single"), "my-plugin").is_err());
}

#[test]
fn definitions_deserialize_from_plugin_json() {
    let def: SettingDef = serde_json::from_value(json!({
        "id": "latex-tools.shell-escape",
        "type": "enum",
        "values": ["off", "restricted"],
        "default": "off",
        "description": "Shell escape policy.",
        "requires-restart": true
    }))
    .expect("parses");
    assert_eq!(def.scope, Scope::User);
    assert!(def.requires_restart);
}

fn any_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<i64>().prop_map(|i| json!(i)),
        (-100i64..100).prop_map(|i| json!(i)),
        any::<bool>().prop_map(|b| json!(b)),
        "[a-z-]{0,8}".prop_map(|s| json!(s)),
        Just(json!("dark")),
        Just(json!("typst")),
        (0.0f64..4.0).prop_map(|f| json!(f)),
        Just(Value::Null),
    ]
}

proptest! {
    /// The merged value always equals what a straightforward oracle computes:
    /// the project value if valid and allowed, else the user value if valid,
    /// else the default; and it is always valid.
    #[test]
    fn layered_merge_matches_oracle(
        user in proptest::collection::btree_map(
            proptest::sample::select(Registry::with_core().iter().map(|d| d.id.clone()).collect::<Vec<_>>()),
            any_value(), 0..10),
        project in proptest::collection::btree_map(
            proptest::sample::select(Registry::with_core().iter().map(|d| d.id.clone()).collect::<Vec<_>>()),
            any_value(), 0..10),
    ) {
        let reg = Registry::with_core();
        let resolved = layers::resolve_all(&reg, &user, &project);
        for def in reg.iter() {
            let got = &resolved[&def.id];
            prop_assert!(def.validate(&got.value).is_ok());
            let project_ok = if def.scope == Scope::Project {
                project.get(&def.id).and_then(|v| def.validate(v).ok())
            } else {
                None
            };
            let user_ok = user.get(&def.id).and_then(|v| def.validate(v).ok());
            let expected = match (project_ok, user_ok) {
                (Some(v), _) => (v, layers::Source::Project),
                (None, Some(v)) => (v, layers::Source::User),
                _ => (def.default.clone(), layers::Source::Default),
            };
            prop_assert_eq!((got.value.clone(), got.source), expected);
        }
    }

    /// Removing the project layer yields exactly the user view.
    #[test]
    fn empty_project_layer_is_the_user_view(
        user in proptest::collection::btree_map(
            proptest::sample::select(Registry::with_core().iter().map(|d| d.id.clone()).collect::<Vec<_>>()),
            any_value(), 0..10),
    ) {
        let reg = Registry::with_core();
        let with_empty = layers::resolve_all(&reg, &user, &Default::default());
        for r in with_empty.values() {
            prop_assert_ne!(r.source, layers::Source::Project);
        }
    }
}
