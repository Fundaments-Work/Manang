//! The writing plugin driven through the kernel, as the shell will.

use std::sync::Arc;

use manang_kernel::{Hook, Kernel, KernelError, Payload, Tier};
use manang_writing::{manifest, plugin};
use serde_json::{Value, json};
use tempfile::TempDir;

fn kernel() -> (Kernel, TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let kernel = Kernel::new();
    kernel
        .register(
            manifest().expect("manifest is valid"),
            Arc::new(plugin(dir.path().to_path_buf())),
        )
        .expect("registers");
    (kernel, dir)
}

fn call(k: &Kernel, command: &str, args: Value) -> Result<Value, KernelError> {
    match k.invoke(command, Payload::Json(args))? {
        Payload::Json(v) => Ok(v),
        other => panic!("expected JSON, got {other:?}"),
    }
}

fn ok(k: &Kernel, command: &str, args: Value) -> Value {
    call(k, command, args).unwrap_or_else(|e| panic!("{command}: {e}"))
}

#[test]
fn manifest_is_native_and_every_command_is_handled() {
    let m = manifest().expect("valid");
    assert_eq!(m.plugin.name, "writing");
    assert_eq!(m.plugin.tier, Tier::Native);
    assert_eq!(m.contributes.hooks, vec![Hook::OnLint]);
    let (k, _dir) = kernel();
    for command in k.commands() {
        // Every declared command is routed: a bad-argument error means the
        // plugin recognised it, `UnknownCommand` would mean a gap.
        let result = call(&k, &command.id, json!({}));
        if let Err(KernelError::Plugin(manang_kernel::PluginError::UnknownCommand(c))) = &result {
            panic!("`{c}` is declared but not handled");
        }
    }
    assert!(matches!(
        call(&k, "writing.nope", json!({})),
        Err(KernelError::UnknownCommand(_))
    ));
}

#[test]
fn format_document_and_range() {
    let (k, _dir) = kernel();
    let out = ok(
        &k,
        "writing.format.document",
        json!({"source": "#let f(x,y)=x+y\n", "language": "typst"}),
    );
    assert_eq!(out["changed"], true);
    assert!(
        out["formatted"]
            .as_str()
            .is_some_and(|s| s.contains("#let f(x, y) = x + y"))
    );

    let out = ok(
        &k,
        "writing.format.document",
        json!({"source": "a  \n\n\n\nb\n", "language": "markdown"}),
    );
    assert_eq!(out["formatted"], "a  \n\nb\n");

    let out = ok(
        &k,
        "writing.format.range",
        json!({"source": "#let a=1\n#let b=2\n", "from": 0, "to": 8, "language": "typst"}),
    );
    assert_eq!(out["range"]["start"], 0);
    assert!(
        out["formatted"]
            .as_str()
            .is_some_and(|s| s.contains("#let b=2"))
    );

    let err = call(
        &k,
        "writing.format.document",
        json!({"source": "\\section{A}", "language": "latex"}),
    )
    .expect_err("latex is unsupported");
    assert!(err.to_string().contains("unsupported"), "{err}");
    assert!(
        call(
            &k,
            "writing.format.range",
            json!({"source": "ab", "from": 1, "to": 9, "language": "markdown"})
        )
        .is_err()
    );
}

#[test]
fn spell_check_and_user_dictionary_persist() {
    let (k, dir) = kernel();
    let text = "The quik fox met Manang.";
    let out = ok(
        &k,
        "writing.spell.check",
        json!({"text": text, "language": "en-US"}),
    );
    assert_eq!(out["language"], "en_US");
    let words: Vec<_> = out["misspelled"]
        .as_array()
        .expect("array")
        .iter()
        .map(|m| m["word"].as_str().expect("word"))
        .collect();
    assert_eq!(words, ["quik", "Manang"]);
    assert!(
        out["misspelled"][0]["suggestions"]
            .as_array()
            .is_some_and(|s| !s.is_empty() && s.len() <= 5)
    );

    let added = ok(&k, "writing.spell.add-word", json!({"word": "Manang"}));
    assert_eq!(added["added"], true);
    let out = ok(
        &k,
        "writing.spell.check",
        json!({"text": text, "max_suggestions": 0}),
    );
    assert_eq!(out["count"], 1);
    assert_eq!(
        ok(&k, "writing.spell.words", json!({}))["words"],
        json!(["Manang"])
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("user-dictionary.txt")).expect("file"),
        "Manang\n"
    );

    // A fresh plugin on the same directory sees the persisted word.
    let again = Kernel::new();
    again
        .register(
            manifest().expect("manifest"),
            Arc::new(plugin(dir.path().to_path_buf())),
        )
        .expect("registers");
    assert_eq!(
        ok(&again, "writing.spell.words", json!({}))["words"],
        json!(["Manang"])
    );
    let removed = ok(
        &again,
        "writing.spell.remove-word",
        json!({"word": "Manang"}),
    );
    assert_eq!(removed["removed"], true);
    assert_eq!(
        ok(&again, "writing.spell.words", json!({}))["words"],
        json!([])
    );

    assert!(
        call(
            &k,
            "writing.spell.check",
            json!({"text": "x", "language": "tlh"})
        )
        .is_err()
    );
    let langs = ok(&k, "writing.spell.languages", json!({}));
    assert_eq!(langs["default"], "en_US");
}

#[test]
fn spell_check_skips_non_prose_and_honours_ignore_regions() {
    let (k, _dir) = kernel();
    let text = "A wrongg word $xyzzy$ #cite(<qwrt>) and ignoredd here";
    let out = ok(
        &k,
        "writing.spell.check",
        json!({"text": text, "syntax": "typst", "ignore_regions": [{"start": 40, "end": 48}]}),
    );
    let words: Vec<_> = out["misspelled"]
        .as_array()
        .expect("array")
        .iter()
        .map(|m| m["word"].as_str().expect("w"))
        .collect();
    assert_eq!(words, ["wrongg"], "{out}");
}

#[test]
fn lint_command_and_on_lint_hook_share_a_shape() {
    let (k, _dir) = kernel();
    let text = "This is is a very long test.  Results were shown.";
    let cmd = ok(
        &k,
        "writing.lint.prose",
        json!({"text": text, "syntax": "plain", "file": "/main.typ"}),
    );
    assert_eq!(cmd["source"], "writing");
    let diags = cmd["diagnostics"].as_array().expect("array");
    assert!(diags.len() >= 4, "{cmd}");
    for d in diags {
        for key in [
            "severity", "message", "file", "line", "column", "range", "hints", "trace",
        ] {
            assert!(d.get(key).is_some(), "missing `{key}` in {d}");
        }
        assert_eq!(d["file"], "/main.typ");
    }

    let hooked = k
        .run_hook(
            Hook::OnLint,
            Payload::Json(json!({"source": text, "syntax": "plain", "file": "/main.typ"})),
        )
        .expect("hook");
    let (plugin_name, Payload::Json(via_hook)) = &hooked[0] else {
        panic!("expected JSON from hook");
    };
    assert_eq!(plugin_name, "writing");
    assert_eq!(*via_hook, cmd);

    let selected = ok(
        &k,
        "writing.lint.prose",
        json!({"text": text, "syntax": "plain", "rules": ["repeated-word"]}),
    );
    assert_eq!(selected["diagnostics"].as_array().map(Vec::len), Some(1));
    assert!(
        call(
            &k,
            "writing.lint.prose",
            json!({"text": text, "rules": ["nope"]})
        )
        .is_err()
    );

    let spelled = ok(
        &k,
        "writing.lint.prose",
        json!({"text": "A wrongg word.", "spell": true, "rules": []}),
    );
    assert_eq!(spelled["diagnostics"][0]["code"], "spelling");
}

#[test]
fn symbols_snippets_and_stats() {
    let (k, dir) = kernel();
    let hits = ok(
        &k,
        "writing.symbols.search",
        json!({"query": "for all", "syntax": "latex", "limit": 3}),
    );
    assert_eq!(hits["symbols"][0]["insert"], "\\forall");
    let hits = ok(
        &k,
        "writing.symbols.search",
        json!({"query": "alpha", "syntax": "typst"}),
    );
    assert_eq!(hits["symbols"][0]["insert"], "alpha");
    assert_eq!(hits["symbols"][0]["preview"], "\u{3b1}");

    let list = ok(&k, "writing.snippets.list", json!({"syntax": "typst"}));
    assert!(list["snippets"].as_array().is_some_and(|s| s.len() >= 6));
    let got = ok(
        &k,
        "writing.snippets.get",
        json!({"trigger": "fig", "syntax": "latex"}),
    );
    assert_eq!(got["trigger"], "figure");
    let exp = ok(
        &k,
        "writing.snippets.expand",
        json!({"trigger": "cite", "syntax": "latex"}),
    );
    assert_eq!(exp["text"], "\\cite{key}");
    assert_eq!(
        exp["tab_stops"][0]["ranges"][0],
        json!({"start": 6, "end": 9})
    );
    assert_eq!(exp["final_cursor"], 10);

    std::fs::write(
        dir.path().join("snippets.toml"),
        "[[snippet]]\ntrigger = \"mine\"\nsyntax = \"markdown\"\nbody = \"## ${1:Title}\"\n",
    )
    .expect("write");
    let exp = ok(
        &k,
        "writing.snippets.expand",
        json!({"trigger": "mine", "syntax": "markdown"}),
    );
    assert_eq!(exp["text"], "## Title");
    assert!(
        call(
            &k,
            "writing.snippets.expand",
            json!({"trigger": "zzz", "syntax": "typst"})
        )
        .is_err()
    );

    let stats = ok(
        &k,
        "writing.stats.count",
        json!({"text": "= Hi\nSome $x$ words here.", "syntax": "typst"}),
    );
    assert_eq!(stats["words"], 4);
}

#[test]
fn bad_arguments_are_errors_not_panics() {
    let (k, _dir) = kernel();
    for (command, args) in [
        ("writing.spell.check", json!({"text": 5})),
        (
            "writing.spell.check",
            json!({"text": "x", "ignore_regions": "no"}),
        ),
        (
            "writing.symbols.search",
            json!({"query": "a", "syntax": "plain"}),
        ),
        (
            "writing.stats.count",
            json!({"text": "x", "syntax": "klingon"}),
        ),
        (
            "writing.format.range",
            json!({"source": "a", "from": "x", "to": 1, "language": "markdown"}),
        ),
        ("writing.spell.add-word", json!({"word": "two words"})),
    ] {
        assert!(call(&k, command, args.clone()).is_err(), "{command} {args}");
    }
    assert!(k.invoke("writing.stats.count", Payload::Empty).is_err());
}
