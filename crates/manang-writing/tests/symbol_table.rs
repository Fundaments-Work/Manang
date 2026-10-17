//! The symbol table is checked against Typst itself: every Typst insertion
//! must compile in math mode, and every named symbol must resolve to the
//! preview character the table claims.

use std::collections::BTreeMap;

use manang_typst::{CompileRequest, Compiler, CompilerConfig};
use manang_writing::symbols::table;
use typst::foundations::Value;
use typst::{Library, LibraryExt};

fn typst_char(library: &Library, ident: &str) -> Option<String> {
    let mut parts = ident.split('.');
    let base = parts.next()?;
    let sym = match library.global.scope().get("sym")?.read() {
        Value::Module(m) => m.clone(),
        _ => return None,
    };
    let binding = sym
        .scope()
        .get(base)
        .or_else(|| library.math.scope().get(base))?;
    let Value::Symbol(symbol) = binding.read() else {
        return None;
    };
    let mut symbol = symbol.clone();
    for modifier in parts {
        symbol = symbol.modified((), modifier).ok()?;
    }
    // Typst appends U+FE0E (text presentation) to some arrows; it is invisible.
    Some(symbol.get().replace('\u{fe0e}', ""))
}

fn ident(insertion: &str) -> &str {
    let end = insertion
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.'))
        .unwrap_or(insertion.len());
    &insertion[..end]
}

#[test]
fn named_symbols_match_typst() {
    let library = Library::default();
    let mut wrong = Vec::new();
    for entry in table() {
        let Some(typst) = &entry.typst else { continue };
        if matches!(entry.category.as_str(), "construct" | "function" | "space") {
            continue;
        }
        // Letter-likes Typst has no named symbol for are inserted as the
        // character itself.
        if *typst == entry.preview {
            continue;
        }
        match typst_char(&library, typst) {
            Some(c) if c == entry.preview => {}
            Some(c) => wrong.push(format!(
                "{typst}: table says {:?}, Typst has {c:?}",
                entry.preview
            )),
            None => wrong.push(format!("{typst}: not a Typst symbol")),
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn function_names_exist_in_typst_math() {
    let library = Library::default();
    for entry in table().iter().filter(|e| e.category == "function") {
        let Some(typst) = &entry.typst else { continue };
        let known = library.math.scope().get(typst).is_some()
            || library.global.scope().get(typst).is_some();
        assert!(known, "`{typst}` is not known to Typst math");
    }
}

#[test]
fn every_typst_insertion_compiles() {
    let dir = tempfile::tempdir().expect("tempdir");
    let entries: Vec<_> = table().iter().filter_map(|e| e.typst.as_deref()).collect();
    let mut source = String::new();
    for typst in &entries {
        source.push_str(&format!("$ {typst} $\n"));
    }
    let compiler = Compiler::new(CompilerConfig::default()).expect("compiler");
    let report = compiler
        .compile(&CompileRequest {
            main: "main.typ".to_owned(),
            root: Some(dir.path().to_path_buf()),
            sources: BTreeMap::from([("main.typ".to_owned(), source)]),
        })
        .expect("compile runs");
    let errors: Vec<String> = report
        .diagnostics
        .iter()
        .filter(|d| format!("{:?}", d.severity) == "Error")
        .map(|d| {
            let line = d.line.unwrap_or(0);
            format!(
                "`{}` (line {line}): {}",
                entries.get(line.wrapping_sub(1)).unwrap_or(&"?"),
                d.message
            )
        })
        .collect();
    assert!(report.ok && errors.is_empty(), "{}", errors.join("\n"));
    assert!(ident("arrow.r.double + x") == "arrow.r.double");
}
