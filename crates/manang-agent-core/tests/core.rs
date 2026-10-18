use futures::StreamExt;
use futures::executor::block_on;
use manang_agent_core::*;
use proptest::prelude::*;

fn request() -> ChatRequest {
    ChatRequest {
        model: "claude-sonnet-5-5".into(),
        system: Some("You edit scientific documents.".into()),
        messages: vec![Message::user_text("hello").unwrap()],
        tools: vec![],
        max_tokens: 1024,
        cache_breakpoints: vec![],
    }
}

// ---- blocks and transcripts -------------------------------------------------

#[test]
fn block_keeps_exact_bytes_and_kind() {
    let raw = r#"{ "type":"thinking",  "thinking":"aéb", "signature":"Eq==" }"#;
    let b = ContentBlock::from_json_str(raw).unwrap();
    assert_eq!(b.kind(), BlockKind::Thinking);
    assert_eq!(b.raw_json(), raw);
    assert_eq!(serde_json::to_string(&b).unwrap(), raw);
}

#[test]
fn block_without_type_is_rejected() {
    assert!(ContentBlock::from_json_str(r#"{"text":"x"}"#).is_err());
    assert!(ContentBlock::from_json_str("[1]").is_err());
}

#[test]
fn tool_use_view_decodes() {
    let b =
        ContentBlock::tool_use("t1", "figure__add_shape", &serde_json::json!({"a": 1})).unwrap();
    let v = b.as_tool_use().unwrap();
    assert_eq!(
        (v.id.as_str(), v.name.as_str()),
        ("t1", "figure__add_shape")
    );
    assert_eq!(v.input["a"], 1);
    assert!(b.as_text().is_none());
}

#[test]
fn newline_whitespace_in_raw_block_is_compacted_not_corrupted() {
    let raw =
        "{\"type\":\"thinking\",\n \"thinking\":\"line1\\nline2  x\",\n\"signature\":\"S==\"}";
    let m = Message::new(
        Role::Assistant,
        vec![ContentBlock::from_json_str(raw).unwrap()],
    );
    let line = encode_line(&TranscriptEntry { seq: 0, message: m }).unwrap();
    assert!(!line.contains('\n'));
    let back: ReadLog<TranscriptEntry> = read_all(&line).unwrap();
    let block = &back.entries[0].message.content[0];
    // String contents survive untouched.
    assert!(block.raw_json().contains("line1\\nline2  x"));
    assert!(block.raw_json().contains("\"signature\":\"S==\""));
}

#[test]
fn truncated_tail_is_reported_but_bad_middle_line_is_an_error() {
    let ok = encode_line(&TranscriptEntry {
        seq: 0,
        message: Message::user_text("hi").unwrap(),
    })
    .unwrap();
    let cut = format!("{ok}\n{}", &ok[..ok.len() / 2]);
    let log: ReadLog<TranscriptEntry> = read_all(&cut).unwrap();
    assert_eq!(log.entries.len(), 1);
    assert!(log.truncated_tail);
    let bad = format!("{ok}\nnot json\n{ok}\n");
    assert!(matches!(
        read_all::<TranscriptEntry>(&bad),
        Err(CoreError::BadLine { line: 2, .. })
    ));
}

#[test]
fn append_writes_one_line_per_entry() {
    let mut sink = Vec::new();
    for seq in 0..3 {
        append(
            &mut sink,
            &TranscriptEntry {
                seq,
                message: Message::user_text("x").unwrap(),
            },
        )
        .unwrap();
    }
    let text = String::from_utf8(sink).unwrap();
    assert_eq!(text.lines().count(), 3);
    assert!(text.ends_with('\n'));
}

fn block_strategy() -> impl Strategy<Value = ContentBlock> {
    // Whitespace (spaces and tabs only) between tokens, odd escapes and
    // unicode inside strings: all must come back unchanged.
    let ws = "[ \t]{0,3}";
    let s = "[a-zA-Z0-9+/=\u{e9}\u{4e2d} ]{0,24}";
    (ws, ws, s, s).prop_map(|(w1, w2, think, sig)| {
        let raw = format!(
            "{{{w1}\"type\":{w2}\"thinking\",\"thinking\":\"{think}\",\"signature\":\"{sig}\"}}"
        );
        ContentBlock::from_json_str(&raw).unwrap()
    })
}

proptest! {
    #[test]
    fn transcript_round_trips_byte_exact(blocks in proptest::collection::vec(block_strategy(), 1..6), text in "[ -~]{0,40}") {
        let mut messages = vec![Message::user_text(&text).unwrap()];
        messages.push(Message::new(Role::Assistant, blocks));
        let mut sink = Vec::new();
        for (i, m) in messages.iter().enumerate() {
            append(&mut sink, &TranscriptEntry { seq: i as u64, message: m.clone() }).unwrap();
        }
        let text = String::from_utf8(sink.clone()).unwrap();
        let log: ReadLog<TranscriptEntry> = read_all(&text).unwrap();
        prop_assert!(!log.truncated_tail);
        prop_assert_eq!(log.entries.len(), messages.len());
        for (entry, original) in log.entries.iter().zip(&messages) {
            prop_assert_eq!(&entry.message, original);
            for (a, b) in entry.message.content.iter().zip(&original.content) {
                prop_assert_eq!(a.raw_json(), b.raw_json());
            }
        }
        // Writing what was read produces the same bytes.
        let mut again = Vec::new();
        for e in &log.entries { append(&mut again, e).unwrap(); }
        prop_assert_eq!(again, sink);
    }

    #[test]
    fn compact_json_is_idempotent(s in "[ -~\n\t]{0,60}") {
        let once = compact_json(&s);
        prop_assert_eq!(compact_json(&once), once);
    }
}

// ---- policy -----------------------------------------------------------------

fn layer_strategy() -> impl Strategy<Value = PolicyLayer> {
    let providers = proptest::option::of(proptest::collection::vec(
        prop_oneof![Just("anthropic"), Just("openai_compat"), Just("ollama")]
            .prop_map(String::from),
        0..4,
    ));
    (
        proptest::option::of(any::<bool>()),
        providers,
        proptest::option::of(
            prop_oneof![Just("https://a"), Just("https://b")].prop_map(String::from),
        ),
        proptest::option::of(any::<bool>()),
        proptest::option::of(any::<bool>()),
        proptest::option::of(0u64..100_000),
    )
        .prop_map(
            |(
                enabled,
                allowed_providers,
                base_url,
                require_audit_log,
                allow_mcp_client,
                max_context_tokens,
            )| PolicyLayer {
                enabled,
                allowed_providers,
                base_url,
                require_audit_log,
                allow_mcp_client,
                max_context_tokens,
            },
        )
}

proptest! {
    #[test]
    fn adding_a_lower_layer_never_loosens(layers in proptest::collection::vec(layer_strategy(), 0..4), extra in layer_strategy()) {
        let before = EffectivePolicy::merge(&layers);
        let mut with_extra = layers.clone();
        with_extra.push(extra);
        let after = EffectivePolicy::merge(&with_extra);
        prop_assert!(after.is_at_most_as_permissive_as(&before));
    }

    #[test]
    fn disabled_above_stays_disabled(layers in proptest::collection::vec(layer_strategy(), 0..4), idx in 0usize..4) {
        let mut layers = layers;
        layers.insert(idx.min(layers.len()), PolicyLayer { enabled: Some(false), ..Default::default() });
        prop_assert!(!EffectivePolicy::merge(&layers).enabled);
    }

    #[test]
    fn provider_allowed_only_if_every_layer_allows(layers in proptest::collection::vec(layer_strategy(), 0..4), p in "anthropic|openai_compat|ollama") {
        let merged = EffectivePolicy::merge(&layers);
        let allowed = merged.check_provider(&p).is_ok();
        let every = layers.iter().all(|l| l.allowed_providers.as_ref().is_none_or(|v| v.contains(&p)));
        prop_assert_eq!(allowed, every);
    }
}

#[test]
fn policy_examples() {
    let enterprise = PolicyLayer {
        allowed_providers: Some(vec!["anthropic".into(), "ollama".into()]),
        base_url: Some("https://gateway.corp".into()),
        require_audit_log: Some(true),
        ..Default::default()
    };
    let user = PolicyLayer {
        allowed_providers: Some(vec!["anthropic".into(), "openai_compat".into()]),
        base_url: Some("https://elsewhere".into()),
        require_audit_log: Some(false),
        enabled: Some(true),
        ..Default::default()
    };
    let p = EffectivePolicy::merge(&[enterprise, user]);
    assert!(p.check_provider("anthropic").is_ok());
    assert!(p.check_provider("ollama").is_err());
    assert!(p.check_provider("openai_compat").is_err());
    assert!(p.check_base_url("https://gateway.corp").is_ok());
    assert!(p.check_base_url("https://elsewhere").is_err());
    assert!(p.require_audit_log);
    assert!(p.check_enabled().is_ok());
}

#[test]
fn policy_layer_rejects_unknown_keys() {
    assert!(serde_json::from_str::<PolicyLayer>(r#"{"enabld": false}"#).is_err());
}

// ---- redaction --------------------------------------------------------------

const SAMPLE: &str = "key sk-ant-api03-AbCdEfGhIjKlMnOpQrStUv and ghp_0123456789abcdefABCDEF0123456789abcd.\n\
AWS AKIAIOSFODNN7EXAMPLE, export API_KEY=supersecretvalue1 ok\n\
Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345\n\
mail alice@example.org from /home/alice/papers/thesis.tex\n\
-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkq\n-----END PRIVATE KEY-----\nplain text sk-short";

#[test]
fn redaction_defaults_snapshot() {
    let r = redact(SAMPLE, &RedactionConfig::default());
    insta::assert_snapshot!(r.text);
    insta::assert_json_snapshot!(r.spans);
}

#[test]
fn redaction_opt_in_snapshot() {
    let cfg = RedactionConfig {
        secrets: true,
        emails: true,
        paths: true,
    };
    let r = redact(SAMPLE, &cfg);
    insta::assert_snapshot!(r.text);
}

#[test]
fn redaction_diff_points_into_the_original() {
    let r = redact(SAMPLE, &RedactionConfig::default());
    assert!(!r.spans.is_empty());
    for s in &r.spans {
        assert!(SAMPLE.is_char_boundary(s.start) && SAMPLE.is_char_boundary(s.end));
    }
    assert!(!r.text.contains("AbCdEfGhIjKl"));
    assert!(!r.text.contains("MIIEvQ"));
    assert!(r.text.contains("alice@example.org"), "emails are opt-in");
    assert!(r.text.contains("sk-short"));
}

proptest! {
    #[test]
    fn redaction_is_idempotent(s in "[ -~\n\u{e9}]{0,200}", emails in any::<bool>(), paths in any::<bool>()) {
        let cfg = RedactionConfig { secrets: true, emails, paths };
        let once = redact(&s, &cfg);
        let twice = redact(&once.text, &cfg);
        prop_assert_eq!(&twice.text, &once.text);
        prop_assert!(twice.spans.is_empty());
    }

    #[test]
    fn redaction_idempotent_on_secretish_input(
        prefix in prop_oneof![Just("sk-"), Just("ghp_"), Just("api_key="), Just("Bearer "), Just("/home/"), Just("eyJ")],
        body in "[A-Za-z0-9._@-]{0,50}",
        tail in "[ .,]{0,2}",
    ) {
        let s = format!("x {prefix}{body}{tail} y@z.io");
        let cfg = RedactionConfig { secrets: true, emails: true, paths: true };
        let once = redact(&s, &cfg);
        prop_assert_eq!(redact(&once.text, &cfg).text, once.text);
    }
}

// ---- names ------------------------------------------------------------------

proptest! {
    #[test]
    fn tool_names_round_trip(segments in proptest::collection::vec("[a-z][a-z0-9-]{0,8}(_[a-z0-9]{1,4}){0,2}", 1..4)) {
        let name = segments.join(".");
        let wire = to_wire(&name).unwrap();
        prop_assert!(wire.len() <= 64);
        prop_assert!(wire.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        prop_assert_eq!(from_wire(&wire), name.clone());
        let map = ToolNameMap::new([name.as_str()]).unwrap();
        prop_assert_eq!(map.internal(&wire), Some(name.as_str()));
        prop_assert_eq!(map.wire(&name), Some(wire.as_str()));
    }
}

#[test]
fn tool_name_rules() {
    assert_eq!(to_wire("figure.add_shape").unwrap(), "figure__add_shape");
    for bad in ["", "a..b", "a.b__c", "a_.b", "_a.b", "has space.x", "a.b.é"] {
        assert!(to_wire(bad).is_err(), "{bad}");
    }
    assert!(to_wire(&format!("a.{}", "x".repeat(70))).is_err());
    let map = ToolNameMap::new(["figure.add_shape", "citations.verify"]).unwrap();
    assert_eq!(map.internal("invented__tool"), None);
}

// ---- price, audit, mock ------------------------------------------------------

#[test]
fn cost_estimate_snapshot() {
    let table = PriceTable::builtin();
    let usage = Usage {
        input: 12_000,
        output: 800,
        cache_read: 40_000,
        cache_write: 2_000,
    };
    let mut out = Vec::new();
    for model in [
        "claude-fable-5-1",
        "claude-opus-5-5",
        "claude-sonnet-5-5",
        "claude-haiku-5-5",
    ] {
        let c = table.estimate(model, &usage).unwrap();
        out.push(format!(
            "{model}: {} ({} micro-USD, estimate={})",
            c.display(),
            c.micro_usd,
            c.estimate
        ));
    }
    insta::assert_snapshot!(out.join("\n"));
    assert!(table.estimate("unknown-model", &usage).is_none());
}

#[test]
fn user_can_override_prices() {
    let mut t = PriceTable::builtin();
    t.set("claude-sonnet-5-5", Price::new(1_000_000, 1_000_000, 0, 0));
    let u = Usage {
        input: 1_000_000,
        ..Usage::default()
    };
    assert_eq!(
        t.estimate("claude-sonnet-5-5", &u).unwrap().micro_usd,
        1_000_000
    );
}

#[test]
fn audit_line_snapshot() {
    let entry = AuditEntry {
        ts_ms: 1_760_000_000_000,
        session: "s-1".into(),
        event: AuditEvent::Request {
            provider: "anthropic".into(),
            model: "claude-sonnet-5-5".into(),
            request_id: Some("req_01".into()),
            payload_sha256: payload_hash(b"{\"model\":\"x\"}"),
            payload_bytes: 13,
            redactions: 2,
            payload: None,
        },
    };
    let line = encode_line(&entry).unwrap();
    insta::assert_snapshot!(line);
    let back: ReadLog<AuditEntry> = read_all(&line).unwrap();
    assert_eq!(back.entries, vec![entry]);
}

#[test]
fn payload_hash_known_vector() {
    assert_eq!(
        payload_hash(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn request_validation() {
    assert!(request().validate().is_ok());
    let mut r = request();
    r.cache_breakpoints = vec![CacheBreakpoint::System; 5];
    assert!(r.validate().is_err());
    let mut r = request();
    r.cache_breakpoints = vec![CacheBreakpoint::Message(3)];
    assert!(r.validate().is_err());
    let mut r = request();
    r.max_tokens = 0;
    assert!(r.validate().is_err());
}

#[test]
fn mock_provider_replays_cassette_turns() {
    let cassette = Cassette::from_json(include_str!("cassettes/tool_turn.json")).unwrap();
    let mock = MockProvider::new(cassette.clone());
    let req = request();
    let first: Vec<ProviderEvent> = block_on(mock.stream(&req).collect());
    assert_eq!(first, cassette.turns[0]);
    assert!(matches!(first[1], ProviderEvent::Block { .. }));
    let second: Vec<ProviderEvent> = block_on(mock.stream(&req).collect());
    assert_eq!(second, cassette.turns[1]);
    let third: Vec<ProviderEvent> = block_on(mock.stream(&req).collect());
    match &third[..] {
        [ProviderEvent::Error { error }] => assert_eq!(error.code, "cassette_exhausted"),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(mock.requests().len(), 3);
    // The cassette re-serialises to something that parses back identically.
    assert_eq!(
        Cassette::from_json(&cassette.to_json().unwrap()).unwrap(),
        cassette
    );
}

// ---- presets and DeepSeek prices ----------------------------------------------

#[test]
fn presets_are_consistent() {
    let all = presets();
    let ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
    for id in [
        "anthropic",
        "openai",
        "deepseek",
        "openrouter",
        "groq",
        "ollama",
        "llama_cpp",
        "lm_studio",
        "custom",
    ] {
        assert!(ids.contains(&id), "{id}");
    }
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "ids are unique");
    let ds = preset_by_id("deepseek").unwrap();
    assert_eq!(ds.base_url.as_deref(), Some("https://api.deepseek.com"));
    assert!(ds.reasoning_passback && ds.key_required);
    let ollama = preset_by_id("ollama").unwrap();
    assert!(!ollama.key_required && ollama.local);
    assert_eq!(
        ollama.base_url.as_deref(),
        Some("http://localhost:11434/v1")
    );
    assert!(preset_by_id("custom").unwrap().base_url.is_none());
    assert_eq!(
        preset_by_id("anthropic").unwrap().models[0],
        "claude-sonnet-5-5"
    );
    let json = serde_json::to_string(&all).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<ProviderPreset>>(&json).unwrap(),
        all
    );
}

#[test]
fn deepseek_cache_hits_are_cheaper_than_misses() {
    let t = PriceTable::builtin();
    let miss = Usage {
        input: 1_000_000,
        ..Usage::default()
    };
    let hit = Usage {
        cache_read: 1_000_000,
        ..Usage::default()
    };
    let m = t.estimate("deepseek-flash", &miss).unwrap().micro_usd;
    let h = t.estimate("deepseek-flash", &hit).unwrap().micro_usd;
    assert_eq!((m, h), (300_000, 6_000));
    assert!(t.estimate("deepseek-v4-pro", &miss).unwrap().micro_usd > m);
}
