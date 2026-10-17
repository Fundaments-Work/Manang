//! Built-in starter projects, one per engine.

use std::fs;
use std::path::Path;

use crate::config::{CONFIG_FILE, Engine, ProjectConfig, TemplateBinding};
use crate::error::ProjectError;
use crate::fsops::atomic_write;

const TYPST_MAIN: &str = include_str!("../starters/main.typ");
const LATEX_MAIN: &str = include_str!("../starters/main.tex");
const MARKDOWN_MAIN: &str = include_str!("../starters/index.md");
const BIB: &str = include_str!("../starters/references.bib");
const GITIGNORE: &str = "# Manang local state (cache, trash, CRDT journal)\n.manang/\n";

fn escape_typst_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '#' | '$' | '*' | '_' | '`' | '@' | '<' | '>' | '[' | ']' | '~' | '/' | '='
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn escape_typst_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_latex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '&' | '%' | '$' | '#' | '_' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

fn render(engine: Engine, title: &str) -> String {
    match engine {
        Engine::Typst => TYPST_MAIN
            .replace("{{title_string}}", &escape_typst_string(title))
            .replace("{{title}}", &escape_typst_markup(title)),
        Engine::Latex => LATEX_MAIN.replace("{{title}}", &escape_latex(title)),
        Engine::Markdown => MARKDOWN_MAIN.replace("{{title}}", title),
    }
}

/// Options for [`create`].
#[derive(Debug, Clone)]
pub struct NewProject<'a> {
    pub name: &'a str,
    pub engine: Engine,
    pub template: Option<&'a str>,
}

/// Scaffold a new project in `dir`, which must not exist or must be empty.
/// Returns the config that was written.
pub fn create(dir: &Path, new: &NewProject<'_>) -> Result<ProjectConfig, ProjectError> {
    let shown = dir.display().to_string();
    let mut config = ProjectConfig::new(new.engine, Some(new.name.to_owned()));
    config.bibliography.paths.push("references.bib".to_owned());
    config.template = new.template.map(|id| TemplateBinding {
        id: id.to_owned(),
        version: None,
    });
    config.validate()?;
    let manifest = config.to_toml()?;

    if dir.exists() {
        if !dir.is_dir() {
            return Err(ProjectError::NotADirectory(shown));
        }
        let occupied = fs::read_dir(dir)
            .map_err(|e| ProjectError::io(format!("reading {shown}"), e))?
            .next()
            .is_some();
        if occupied {
            return Err(ProjectError::Exists(format!(
                "{shown} (the directory is not empty)"
            )));
        }
    } else {
        fs::create_dir_all(dir).map_err(|e| ProjectError::io(format!("creating {shown}"), e))?;
    }

    let main = render(new.engine, new.name);
    let files: [(&str, &str); 4] = [
        (CONFIG_FILE, &manifest),
        (new.engine.default_main(), &main),
        ("references.bib", BIB),
        (".gitignore", GITIGNORE),
    ];
    for (name, content) in files {
        atomic_write(&dir.join(name), content.as_bytes())?;
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_neutralises_markup() {
        assert_eq!(escape_latex("R&D 50% #1"), "R\\&D 50\\% \\#1");
        assert_eq!(escape_typst_markup("a*b #c"), "a\\*b \\#c");
        assert_eq!(escape_typst_string("say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn titles_are_rendered_into_each_engine() {
        assert!(render(Engine::Latex, "A & B").contains("\\title{A \\& B}"));
        assert!(render(Engine::Typst, "A \"B\"").contains("title: \"A \\\"B\\\"\""));
        assert!(render(Engine::Markdown, "T").starts_with("# T\n"));
    }
}
