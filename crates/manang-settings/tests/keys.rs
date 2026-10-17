use manang_settings::keybindings::{KeybindingSet, Resolution};
use manang_settings::keys::{Platform, Shortcut, normalize, normalize_for};
use proptest::prelude::*;

const MOD_ALIASES: &[(&str, &[&str])] = &[
    ("Mod", &["mod", "MOD", "Primary"]),
    ("Ctrl", &["ctrl", "Control", "CTRL"]),
    ("Alt", &["alt", "Option", "opt"]),
    ("Shift", &["shift", "SHIFT"]),
    ("Meta", &["meta", "Cmd", "command", "Super", "win"]),
];

const KEYS: &[(&str, &[&str])] = &[
    ("P", &["p", "P"]),
    ("7", &["7"]),
    ("Escape", &["esc", "Escape", "ESCAPE"]),
    ("ArrowUp", &["up", "ArrowUp"]),
    ("F5", &["f5", "F5"]),
    (",", &[","]),
    ("Plus", &["plus"]),
    ("Enter", &["return", "enter"]),
];

/// A random spelling of a shortcut plus its expected canonical form.
fn spelling() -> impl Strategy<Value = (String, String)> {
    (
        proptest::collection::vec(any::<bool>(), 5),
        proptest::collection::vec(0usize..3, 5),
        0..KEYS.len(),
        0usize..2,
        any::<u64>(),
    )
        .prop_map(|(on, alias, key, alias_key, shuffle)| {
            let mut parts: Vec<String> = Vec::new();
            let mut canon: Vec<&str> = Vec::new();
            for (i, (name, aliases)) in MOD_ALIASES.iter().enumerate() {
                if on[i] {
                    canon.push(name);
                    parts.push(aliases[alias[i] % aliases.len()].to_owned());
                }
            }
            // Deterministic shuffle of modifier order.
            let n = parts.len();
            if n > 1 {
                let r = (shuffle as usize) % n;
                parts.rotate_left(r);
            }
            let (kname, kaliases) = KEYS[key];
            parts.push(kaliases[alias_key % kaliases.len()].to_owned());
            canon.push(kname);
            (parts.join("+"), canon.join("+"))
        })
}

proptest! {
    #[test]
    fn any_spelling_normalises_to_the_canonical_form((text, canon) in spelling()) {
        prop_assert_eq!(normalize(&text), Ok(canon));
    }

    #[test]
    fn normalisation_is_idempotent_and_round_trips((text, _) in spelling()) {
        let once = normalize(&text).expect("valid");
        prop_assert_eq!(normalize(&once), Ok(once.clone()));
        let parsed = Shortcut::parse(&once).expect("parses");
        prop_assert_eq!(parsed.to_string(), once);
    }

    #[test]
    fn chords_normalise_per_step((a, ca) in spelling(), (b, cb) in spelling()) {
        prop_assert_eq!(normalize(&format!("{a} {b}")), Ok(format!("{ca} {cb}")));
    }

    #[test]
    fn concrete_form_has_no_mod(
        (text, _) in spelling(),
        platform in prop_oneof![Just(Platform::Mac), Just(Platform::Windows), Just(Platform::Linux)],
    ) {
        let concrete = normalize_for(&text, platform).expect("valid");
        prop_assert!(!concrete.contains("Mod"));
        // Concrete form is stable under normalisation.
        prop_assert_eq!(normalize_for(&concrete, platform), Ok(concrete));
    }

    /// Arbitrary text never panics the parser.
    #[test]
    fn parser_never_panics(s in "\\PC{0,24}") {
        let _ = normalize(&s);
    }

    /// Resolving never panics and a registered default always resolves to
    /// its own command (or a reported conflict) in a matching context.
    #[test]
    fn defaults_resolve_to_themselves(platform in prop_oneof![Just(Platform::Mac), Just(Platform::Linux)]) {
        let set = KeybindingSet::with_core(platform);
        for b in set.bindings() {
            let ctx: std::collections::HashSet<String> = b.when.positive_contexts();
            let got = set.resolve(&ctx, &b.key.to_string()).expect("resolves");
            let ok = match &got {
                Resolution::Matched { command } => *command == b.command,
                Resolution::Conflict { commands } => commands.contains(&b.command),
                _ => false,
            };
            prop_assert!(ok, "{} -> {:?}", b.command, got);
        }
    }
}
