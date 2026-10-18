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

fn render_template_file(engine: Engine, content: &str, title: &str) -> String {
    match engine {
        Engine::Typst => content
            .replace("{{title_string}}", &escape_typst_string(title))
            .replace("{{title}}", &escape_typst_markup(title)),
        Engine::Latex => content.replace("{{title}}", &escape_latex(title)),
        Engine::Markdown => content.replace("{{title}}", title),
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

    let (config, files_to_write): (ProjectConfig, Vec<(String, String)>) = if let Some(template_id) =
        new.template
        && let Some(tmpl) = crate::template::find_template(template_id)
    {
        let mut cfg = ProjectConfig::new(tmpl.manifest.engine, Some(new.name.to_owned()));
        cfg.project.main = Some(tmpl.manifest.main.clone());
        cfg.template = Some(TemplateBinding {
            id: tmpl.manifest.id.clone(),
            version: None,
        });
        let has_bib = tmpl.files.iter().any(|(name, _)| *name == "references.bib");
        if has_bib {
            cfg.bibliography.paths.push("references.bib".to_owned());
        }
        cfg.validate()?;
        let manifest = cfg.to_toml()?;

        let mut list = vec![
            (CONFIG_FILE.to_owned(), manifest),
            (".gitignore".to_owned(), GITIGNORE.to_owned()),
        ];
        for (fname, content) in tmpl.files {
            if *fname == "template.toml" {
                continue;
            }
            let rendered = render_template_file(tmpl.manifest.engine, content, new.name);
            list.push(((*fname).to_owned(), rendered));
        }
        (cfg, list)
    } else {
        let mut cfg = ProjectConfig::new(new.engine, Some(new.name.to_owned()));
        cfg.bibliography.paths.push("references.bib".to_owned());
        cfg.template = new.template.map(|id| TemplateBinding {
            id: id.to_owned(),
            version: None,
        });
        cfg.validate()?;
        let manifest = cfg.to_toml()?;
        let main = render(new.engine, new.name);
        let list = vec![
            (CONFIG_FILE.to_owned(), manifest),
            (new.engine.default_main().to_owned(), main),
            ("references.bib".to_owned(), BIB.to_owned()),
            (".gitignore".to_owned(), GITIGNORE.to_owned()),
        ];
        (cfg, list)
    };

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

    for (name, content) in &files_to_write {
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
