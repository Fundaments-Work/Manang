//! Rules that add a package: an undefined command or environment.

use crate::engine::Draft;
use crate::types::{Confidence, DiagnosticIn};

use super::Env;
use super::scan::commands;
use super::tables::{command_packages, environment_packages, is_theorem_like};

const RULE_COMMAND: &str = "latex.missing-package";
const RULE_ENVIRONMENT: &str = "latex.missing-environment-package";
const RULE_THEOREM: &str = "latex.undeclared-theorem";

/// `Undefined control sequence` (with the token when the log named it).
pub(super) fn undefined_command(env: &Env<'_>, index: usize, d: &DiagnosticIn) -> Vec<Draft> {
    if !d.message.starts_with("Undefined control sequence") {
        return Vec::new();
    }
    let named = named_token(d);
    let (tokens, exact) = match named {
        Some(token) => (vec![token], true),
        None => candidates_on_line(env, d),
    };
    let mut out = Vec::new();
    for token in tokens {
        if env.pre.defined.contains(&token) {
            continue;
        }
        let packages = command_packages(&token);
        // Loaded already, yet undefined: the cause is not a missing package.
        if packages.is_empty() || packages.iter().any(|p| env.pre.loaded(p)) {
            continue;
        }
        let single = packages.len() == 1;
        for package in packages {
            let confidence = if single && exact {
                Confidence::Certain
            } else {
                Confidence::Likely
            };
            let mut why = format!(
                "`{token}` is defined by the `{package}` package, which this document does not load."
            );
            if !single {
                why.push_str(" Another package also provides it; pick the one you use.");
            }
            out.extend(add_package(
                env,
                index,
                package,
                &why,
                confidence,
                RULE_COMMAND,
            ));
        }
    }
    out
}

/// `LaTeX Error: Environment NAME undefined`.
pub(super) fn undefined_environment(env: &Env<'_>, index: usize, d: &DiagnosticIn) -> Vec<Draft> {
    let Some(name) = environment_name(&d.message) else {
        return Vec::new();
    };
    if is_theorem_like(name) {
        return theorem(env, index, name).into_iter().collect();
    }
    let packages = environment_packages(name);
    if packages.is_empty() || packages.iter().any(|p| env.pre.loaded(p)) {
        return Vec::new();
    }
    let single = packages.len() == 1;
    let mut out = Vec::new();
    for package in packages {
        let confidence = if single {
            Confidence::Certain
        } else {
            Confidence::Likely
        };
        let why = format!(
            "The `{name}` environment is defined by the `{package}` package, which this document does not load."
        );
        out.extend(add_package(
            env,
            index,
            package,
            &why,
            confidence,
            RULE_ENVIRONMENT,
        ));
    }
    out
}

fn environment_name(message: &str) -> Option<&str> {
    let rest = message.split("Environment ").nth(1)?;
    let name = rest.split(" undefined").next()?;
    let name = name.trim_matches(|c| c == '`' || c == '\'' || c == ' ');
    (!name.is_empty()).then_some(name)
}

/// The token in `Undefined control sequence: \foo`, or in a backticked
/// hint.
fn named_token(d: &DiagnosticIn) -> Option<String> {
    let from_message = d.message.split_once(": ").map(|(_, t)| t.trim());
    from_message
        .filter(|t| {
            t.starts_with('\\') && t[1..].chars().all(|c| c.is_ascii_alphabetic() || c == '@')
        })
        .map(str::to_owned)
}

/// Known commands on the diagnostic's line (or the whole document without a
/// line); exact when only one is a candidate.
fn candidates_on_line(env: &Env<'_>, d: &DiagnosticIn) -> (Vec<String>, bool) {
    let masked = env.masked.as_str();
    let range = d
        .line
        .and_then(|line| env.ctx.lines.range(env.text(), line))
        .unwrap_or((0, masked.len()));
    let mut found: Vec<String> = Vec::new();
    for (_, name) in commands(&masked[range.0..range.1]) {
        let known = !command_packages(name).is_empty() && !env.pre.defined.contains(name);
        if known && !found.iter().any(|f| f == name) {
            found.push(name.to_owned());
        }
    }
    // The log points at the line where TeX noticed, which can follow the
    // command by a line when its argument wraps: look one line back.
    if found.is_empty()
        && let Some(line) = d.line.filter(|l| *l > 1)
        && let Some((s, e)) = env.ctx.lines.range(env.text(), line - 1)
    {
        for (_, name) in commands(&masked[s..e]) {
            let known = !command_packages(name).is_empty() && !env.pre.defined.contains(name);
            if known && !found.iter().any(|f| f == name) {
                found.push(name.to_owned());
            }
        }
    }
    let exact = found.len() == 1 && d.line.is_some();
    (found, exact)
}

/// A fix that adds `\usepackage{package}`; `None` when there is nowhere to put
/// it (a file with no `\documentclass` and no packages).
fn add_package(
    env: &Env<'_>,
    index: usize,
    package: &str,
    why: &str,
    confidence: Confidence,
    rule: &'static str,
) -> Option<Draft> {
    let pos = env.pre.package_insertion()?;
    let line = format!("\\usepackage{{{package}}}");
    Some(Draft {
        id: format!("latex.package:{package}"),
        label: format!("Add {line}"),
        why: why.to_owned(),
        edits: vec![env.insert_lines_after(pos, std::slice::from_ref(&line))],
        confidence,
        rule,
        diagnostics: vec![index],
        finding: None,
    })
}

/// `\newtheorem` for an environment the document uses but never declared.
fn theorem(env: &Env<'_>, index: usize, name: &str) -> Option<Draft> {
    let pos = env.pre.package_insertion()?;
    if env.masked.contains(&format!("\\newtheorem{{{name}}}"))
        || env.masked.contains(&format!("\\newtheorem*{{{name}}}"))
    {
        return None;
    }
    let mut title = name.to_owned();
    if let Some(first) = title.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    let mut lines = Vec::new();
    if !env.pre.loaded("amsthm") {
        lines.push("\\usepackage{amsthm}".to_owned());
    }
    let shared = name != "theorem" && env.masked.contains("\\newtheorem{theorem}");
    lines.push(if shared {
        format!("\\newtheorem{{{name}}}[theorem]{{{title}}}")
    } else {
        format!("\\newtheorem{{{name}}}{{{title}}}")
    });
    Some(Draft {
        id: format!("latex.theorem:{name}"),
        label: format!("Declare the {name} environment"),
        why: format!(
            "`{name}` needs a `\\newtheorem` declaration; this adds one (numbered, titled \"{title}\")."
        ),
        edits: vec![env.insert_lines_after(pos, &lines)],
        confidence: Confidence::Likely,
        rule: RULE_THEOREM,
        diagnostics: vec![index],
        finding: None,
    })
}
