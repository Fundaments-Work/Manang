//! `format-adapters`, `editors` and `toolbars`: parsing, validation and the
//! kernel's registries (ADR 0015).

use std::sync::Arc;

use manang_kernel::{
    Context, EditorKind, EditorPriority, Kernel, KernelError, Manifest, ManifestError, Payload,
    Plugin, PluginError,
};

struct Nop;

impl Plugin for Nop {
    fn invoke(&self, _: &Context<'_>, _: &str, _: Payload) -> Result<Payload, PluginError> {
        Ok(Payload::Empty)
    }
}

const MARKDOWN: &str = r#"
[plugin]
name = "markdown-format"
version = "0.1.0"
tier = "native"

[[contributes.format-adapters]]
id = "markdown"
title = "Markdown"
extensions = ["md", "markdown"]
document-type = "markdown"
kinds = ["heading", "strong", "list-item"]
commands = ["bold", "heading"]
"#;

const VISUAL: &str = r#"
[plugin]
name = "visual"
version = "0.1.0"
tier = "native"

[[contributes.commands]]
id = "visual.bold"
title = "Bold"

[[contributes.commands]]
id = "visual.italic"
title = "Italic"

[[contributes.editors]]
id = "visual"
kind = "visual"
document-types = ["*"]
title = "Visual"
priority = "optional"

[[contributes.toolbars]]
editor = "visual"

[[contributes.toolbars.items]]
command = "visual.bold"
icon = "bold"
when = "adapter.has(bold)"

[[contributes.toolbars.items]]
command = "visual.italic"
icon = "italic"
"#;

fn parse(src: &str) -> Result<Manifest, ManifestError> {
    Manifest::from_toml(src)
}

#[test]
fn parses_all_three_contribution_points() {
    let m = parse(MARKDOWN).expect("adapter manifest");
    let a = &m.contributes.format_adapters[0];
    assert_eq!(a.id, "markdown");
    assert_eq!(a.extensions, vec!["md", "markdown"]);
    assert_eq!(a.document_type.as_deref(), Some("markdown"));
    assert_eq!(a.commands, vec!["bold", "heading"]);

    let v = parse(VISUAL).expect("visual manifest");
    let e = &v.contributes.editors[0];
    assert_eq!(e.kind, EditorKind::Visual);
    assert_eq!(e.priority, EditorPriority::Optional);
    assert_eq!(e.document_types, vec!["*"]);
    let t = &v.contributes.toolbars[0];
    assert_eq!(t.editor, "visual");
    assert_eq!(t.items.len(), 2);
    assert_eq!(t.items[0].when.as_deref(), Some("adapter.has(bold)"));
}

#[test]
fn a_manifest_without_them_still_parses() {
    let m = parse("[plugin]\nname = \"plain\"\nversion = \"0.1.0\"\n").expect("valid");
    assert!(m.contributes.format_adapters.is_empty());
    assert!(m.contributes.editors.is_empty());
    assert!(m.contributes.toolbars.is_empty());
}

fn with_tail(tail: &str) -> String {
    format!("[plugin]\nname = \"p\"\nversion = \"0.1.0\"\n{tail}")
}

#[test]
fn rejects_malformed_contributions() {
    let cases = [
        // adapters
        "[[contributes.format-adapters]]\nid = \"Bad\"\nextensions = [\"a\"]\n",
        "[[contributes.format-adapters]]\nid = \"x\"\nextensions = []\n",
        "[[contributes.format-adapters]]\nid = \"x\"\nextensions = [\".md\"]\n",
        "[[contributes.format-adapters]]\nid = \"x\"\nextensions = [\"MD\"]\n",
        "[[contributes.format-adapters]]\nid = \"x\"\nextensions = [\"a\"]\nkinds = [\"Bad Kind\"]\n",
        "[[contributes.format-adapters]]\nid = \"x\"\nextensions = [\"a\"]\ncommands = [\"a_b\"]\n",
        "[[contributes.format-adapters]]\nid = \"x\"\nextensions = [\"a\"]\n[[contributes.format-adapters]]\nid = \"x\"\nextensions = [\"b\"]\n",
        "[[contributes.format-adapters]]\nid = \"x\"\nextensions = [\"a\"]\nsurprise = 1\n",
        // editors
        "[[contributes.editors]]\nid = \"e\"\nkind = \"custom\"\n",
        "[[contributes.editors]]\nid = \"e\"\nkind = \"custom\"\nentry = \"../x.html\"\n",
        "[[contributes.editors]]\nid = \"e\"\nkind = \"wysiwyg\"\n",
        "[[contributes.editors]]\nid = \"e\"\nkind = \"visual\"\ndocument-types = [\"Not Ok\"]\n",
        "[[contributes.editors]]\nid = \"e\"\nkind = \"visual\"\n[[contributes.editors]]\nid = \"e\"\nkind = \"source\"\n",
        // toolbars
        "[[contributes.toolbars]]\neditor = \"e\"\n",
        "[[contributes.toolbars]]\neditor = \"e\"\n[[contributes.toolbars.items]]\ncommand = \"p.undeclared\"\n",
        "[[contributes.commands]]\nid = \"p.a\"\ntitle = \"t\"\n[[contributes.toolbars]]\neditor = \"e\"\n[[contributes.toolbars.items]]\ncommand = \"p.a\"\n[[contributes.toolbars.items]]\ncommand = \"p.a\"\n",
        "[[contributes.commands]]\nid = \"p.a\"\ntitle = \"t\"\n[[contributes.toolbars]]\neditor = \"e\"\n[[contributes.toolbars.items]]\ncommand = \"p.a\"\nicon = \"Bold Icon\"\n",
    ];
    for tail in cases {
        assert!(
            parse(&with_tail(tail)).is_err(),
            "should be rejected:\n{tail}"
        );
    }
}

#[test]
fn custom_editor_with_an_entry_is_fine() {
    let src = with_tail(
        "[[contributes.editors]]\nid = \"docx\"\nkind = \"custom\"\nentry = \"ui/editor.html\"\ndocument-types = [\"docx\"]\npriority = \"default\"\n",
    );
    let m = parse(&src).expect("valid");
    assert_eq!(
        m.contributes.editors[0].entry.as_deref(),
        Some("ui/editor.html")
    );
    assert_eq!(m.contributes.editors[0].priority, EditorPriority::Default);
}

#[test]
fn kernel_registries_list_what_plugins_contribute() {
    let kernel = Kernel::new();
    kernel
        .register(parse(VISUAL).expect("visual"), Arc::new(Nop))
        .expect("register visual");
    kernel
        .register(parse(MARKDOWN).expect("md"), Arc::new(Nop))
        .expect("register md");

    let adapters = kernel.format_adapters();
    assert_eq!(adapters.len(), 1);
    assert_eq!(adapters[0].plugin, "markdown-format");
    assert_eq!(adapters[0].adapter.id, "markdown");
    assert!(kernel.format_adapter("markdown").is_some());
    assert!(kernel.format_adapter("nope").is_none());

    let editors = kernel.editors();
    assert_eq!(editors.len(), 1);
    assert_eq!(editors[0].plugin, "visual");
    assert_eq!(kernel.toolbars().len(), 1);

    // The shell receives plain JSON.
    let json = serde_json::to_value(&adapters[0]).expect("serialises");
    assert_eq!(json["plugin"], "markdown-format");
    assert_eq!(json["document-type"], "markdown");
    assert_eq!(json["extensions"][1], "markdown");
}

#[test]
fn ids_are_unique_across_plugins_and_registration_is_atomic() {
    let kernel = Kernel::new();
    kernel
        .register(parse(MARKDOWN).expect("md"), Arc::new(Nop))
        .expect("first");
    let other = MARKDOWN.replace("markdown-format", "other-format");
    let err = kernel
        .register(parse(&other).expect("parses"), Arc::new(Nop))
        .expect_err("same adapter id");
    assert!(matches!(
        err,
        KernelError::ContributionTaken {
            what: "format adapter",
            ..
        }
    ));
    assert!(
        kernel.manifest("other-format").is_none(),
        "nothing registered"
    );
    assert_eq!(kernel.format_adapters().len(), 1);
}

#[test]
fn unregister_and_replace_release_contributions() {
    let kernel = Kernel::new();
    kernel
        .register(parse(MARKDOWN).expect("md"), Arc::new(Nop))
        .expect("register");
    // Hot reload to a version that renames the adapter frees the old id.
    let renamed = MARKDOWN.replace("id = \"markdown\"", "id = \"markdown2\"");
    kernel
        .replace(parse(&renamed).expect("md2"), Arc::new(Nop))
        .expect("replace");
    assert_eq!(kernel.format_adapters()[0].adapter.id, "markdown2");
    let again = MARKDOWN.replace("markdown-format", "third-format");
    kernel
        .register(parse(&again).expect("third"), Arc::new(Nop))
        .expect("the old id is free");

    kernel.unregister("markdown-format").expect("unregister");
    let ids: Vec<_> = kernel
        .format_adapters()
        .into_iter()
        .map(|a| a.adapter.id)
        .collect();
    assert_eq!(ids, vec!["markdown"]);
}

#[test]
fn replace_may_keep_its_own_ids() {
    let kernel = Kernel::new();
    kernel
        .register(parse(VISUAL).expect("visual"), Arc::new(Nop))
        .expect("register");
    kernel
        .replace(parse(VISUAL).expect("visual"), Arc::new(Nop))
        .expect("a plugin may keep the ids it already owns");
    assert_eq!(kernel.editors().len(), 1);
}
