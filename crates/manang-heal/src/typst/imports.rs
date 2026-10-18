//! `unknown variable` for a name a known `@preview` package exports.

use crate::engine::{ByteEdit, Ctx, Draft};
use crate::text::line_ending;
use crate::types::Confidence;

use super::packages::{Import, Package, providers};
use super::{Placed, unknown_variable};

const RULE: &str = "typst.missing-import";

/// An `#import "@preview/..."` line already in the document.
#[derive(Debug)]
struct ExistingImport {
    package: String,
    /// Byte range of the item list when it is a plain `a, b` list on one
    /// line (not `*`, not parenthesised, not `as`).
    list_end: Option<usize>,
    items: Vec<String>,
    /// Imports everything (`: *`) or only the module.
    wildcard: bool,
}

pub(super) fn missing_imports(ctx: &Ctx<'_>, placed: &[Placed<'_>]) -> Vec<Draft> {
    let text = ctx.text;
    let existing = existing_imports(text);
    let defined = super::names::defined_names(text);
    // Group the unknown names per providing package, keeping first-use order.
    struct Group {
        package: &'static Package,
        how: Import,
        names: Vec<String>,
        diagnostics: Vec<usize>,
        /// Another package provides one of these names.
        ambiguous: bool,
    }
    let mut groups: Vec<Group> = Vec::new();
    for p in placed {
        let Some(name) = unknown_variable(&p.diagnostic.message) else {
            continue;
        };
        if defined.contains(name) || text.get(p.from..p.to) != Some(name) {
            continue;
        }
        let candidates = providers(name);
        let ambiguous = candidates.len() > 1;
        for (package, how) in candidates {
            // Already imported wholesale: the name is not missing.
            if existing
                .iter()
                .any(|e| e.package == package.name && e.wildcard)
            {
                continue;
            }
            match groups
                .iter_mut()
                .find(|g| g.package.name == package.name && same_kind(g.how, how))
            {
                Some(g) => {
                    if !g.names.iter().any(|n| n == name) {
                        g.names.push(name.to_owned());
                    }
                    if !g.diagnostics.contains(&p.index) {
                        g.diagnostics.push(p.index);
                    }
                    g.ambiguous |= ambiguous;
                }
                None => groups.push(Group {
                    package,
                    how,
                    names: vec![name.to_owned()],
                    diagnostics: vec![p.index],
                    ambiguous,
                }),
            }
        }
    }
    groups
        .into_iter()
        .map(|mut g| {
            let confidence = if g.ambiguous {
                Confidence::Likely
            } else {
                Confidence::Certain
            };
            if g.how == Import::Named {
                // Typst stops at the first unknown name of a call, so the rest
                // of the package's names the document uses are missing too.
                for name in siblings(text, g.package, &defined, &existing, &g.names) {
                    g.names.push(name);
                }
            }
            build(
                ctx,
                &existing,
                g.package,
                g.how,
                &g.names,
                g.diagnostics,
                confidence,
            )
        })
        .collect()
}

/// Other names of `package` the document calls (`node(..)`, `draw.line`)
/// without defining or importing them.
fn siblings(
    text: &str,
    package: &Package,
    defined: &std::collections::BTreeSet<String>,
    existing: &[ExistingImport],
    reported: &[String],
) -> Vec<String> {
    let imported = existing
        .iter()
        .filter(|e| e.package == package.name)
        .flat_map(|e| e.items.iter());
    let imported: Vec<&String> = imported.collect();
    package
        .symbols
        .iter()
        .filter(|(_, how)| *how == Import::Named)
        .map(|(name, _)| *name)
        .filter(|name| {
            !reported.iter().any(|r| r == name)
                && !defined.contains(*name)
                && !imported.iter().any(|i| i.as_str() == *name)
                && is_used(text, name)
        })
        .map(str::to_owned)
        .collect()
}

/// `name(` or `name.` with `name` standing alone (not part of a longer
/// identifier, a field access or a string).
fn is_used(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(p) = text[from..].find(name) {
        let at = from + p;
        from = at + name.len();
        let before_ok = at == 0 || {
            let b = bytes[at - 1];
            !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'"' | b'@' | b'/'))
        };
        let after = text[from..].chars().next();
        if before_ok && matches!(after, Some('(' | '.')) {
            return true;
        }
    }
    false
}

fn same_kind(a: Import, b: Import) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

fn build(
    ctx: &Ctx<'_>,
    existing: &[ExistingImport],
    package: &Package,
    how: Import,
    names: &[String],
    diagnostics: Vec<usize>,
    confidence: Confidence,
) -> Draft {
    let eol = line_ending(ctx.text);
    let spec = format!("@preview/{}:{}", package.name, package.version);
    let joined = names.join(", ");
    let (label, line, edit_override): (String, String, Option<ByteEdit>) = match how {
        Import::Module => (
            format!("Import {spec}"),
            format!("#import \"{spec}\""),
            None,
        ),
        Import::Alias(alias) => (
            format!("Import {spec} as {alias}"),
            format!("#import \"{spec}\" as {alias}"),
            None,
        ),
        Import::Named => {
            // Extend the document's own import of this package when it is a
            // plain list; otherwise add a line.
            let extend = existing
                .iter()
                .find(|e| e.package == package.name)
                .and_then(|e| {
                    let at = e.list_end?;
                    let fresh: Vec<&String> =
                        names.iter().filter(|n| !e.items.contains(n)).collect();
                    (!fresh.is_empty()).then(|| {
                        ByteEdit::insert(
                            at,
                            format!(
                                ", {}",
                                fresh
                                    .iter()
                                    .map(|s| s.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        )
                    })
                });
            (
                format!("Import {joined} from {spec}"),
                format!("#import \"{spec}\": {joined}"),
                extend,
            )
        }
        Import::Theme(module) => {
            let line = format!("#import \"{spec}\": *{eol}#import themes.{module}: *");
            (format!("Import the touying {module} theme"), line, None)
        }
    };
    let edit = edit_override.unwrap_or_else(|| {
        let at = import_position(ctx.text);
        let lead = if at > 0 && !ctx.text[..at].ends_with('\n') {
            eol
        } else {
            ""
        };
        ByteEdit::insert(at, format!("{lead}{line}{eol}"))
    });
    let why = match how {
        Import::Module | Import::Alias(_) => format!(
            "`{joined}` is the `{}` package; it has to be imported before use.",
            package.name
        ),
        _ => format!(
            "{} {} from the `{}` package, which this document does not import.",
            quoted(names),
            if names.len() == 1 { "comes" } else { "come" },
            package.name
        ),
    };
    Draft {
        id: format!("typst.import:{}@{}:{joined}", package.name, package.version),
        label,
        why,
        edits: vec![edit],
        confidence,
        rule: RULE,
        diagnostics,
        finding: None,
    }
}

fn quoted(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Where a new import line goes: after the last top-level `#import` line,
/// else after the leading comments, else at the start.
fn import_position(text: &str) -> usize {
    let mut last_import_end: Option<usize> = None;
    let mut leading_end = 0;
    let mut in_leading = true;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#import ") {
            last_import_end = Some(offset + line.len());
            in_leading = false;
        } else if in_leading && (trimmed.starts_with("//") || trimmed.trim().is_empty()) {
            leading_end = offset + line.len();
        } else {
            in_leading = false;
        }
        offset += line.len();
    }
    let at = last_import_end.unwrap_or(leading_end);
    // An import line without a final newline: insert on a new line.
    at.min(text.len())
}

fn existing_imports(text: &str) -> Vec<ExistingImport> {
    const PREFIX: &str = "#import \"@preview/";
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        let indent = body.len() - body.trim_start().len();
        let Some(rest) = body[indent..].strip_prefix(PREFIX) else {
            continue;
        };
        let Some(quote) = rest.find('"') else {
            continue;
        };
        let package = rest[..quote]
            .split(':')
            .next()
            .unwrap_or_default()
            .to_owned();
        let after_quote = indent + PREFIX.len() + quote + 1;
        let after = &body[after_quote..];
        let skipped = after.len() - after.trim_start().len();
        let mut item = ExistingImport {
            package,
            list_end: None,
            items: Vec::new(),
            wildcard: false,
        };
        if let Some(list) = after.trim_start().strip_prefix(':') {
            let colon = line_start + after_quote + skipped;
            let stripped = list.split("//").next().unwrap_or(list).trim_end();
            let content = stripped.trim();
            if content == "*" {
                item.wildcard = true;
            } else if !content.is_empty() && !content.contains(['(', ')', '.']) {
                item.items = content
                    .split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect();
                item.list_end = Some(colon + 1 + stripped.len());
            }
        }
        out.push(item);
    }
    out
}
