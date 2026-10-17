//! `manang.toml`: the project configuration.
//!
//! # Schema
//!
//! ```toml
//! [project]
//! engine = "typst"            # required: "typst" | "latex" | "markdown"
//! name   = "My paper"         # optional display name (default: directory name)
//! main   = "main.typ"         # optional root document, relative to the project
//!                             #   (default per engine: main.typ / main.tex / index.md)
//!
//! [template]                  # optional template binding
//! id      = "ieee-conference" # template identifier
//! version = "1.2.0"           # optional pinned template version
//!
//! [bibliography]              # optional
//! paths = ["references.bib"]  # relative paths; .bib .bibtex .yml .yaml
//! ```
//!
//! Unknown keys are rejected (with a "did you mean" hint) so typos never
//! silently fall back to defaults. All semantic problems are reported
//! together, not one at a time.

use serde::{Deserialize, Serialize};

use crate::error::{ConfigError, Issue};
use crate::paths::lexical_relative;

/// File name of the project configuration, at the project root.
pub const CONFIG_FILE: &str = "manang.toml";

/// Which compiler a project targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Typst,
    Latex,
    Markdown,
}

impl Engine {
    pub const ALL: [Engine; 3] = [Engine::Typst, Engine::Latex, Engine::Markdown];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Typst => "typst",
            Self::Latex => "latex",
            Self::Markdown => "markdown",
        }
    }

    /// File extensions accepted for the main document.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Typst => &["typ"],
            Self::Latex => &["tex"],
            Self::Markdown => &["md", "markdown"],
        }
    }

    pub fn default_main(self) -> &'static str {
        match self {
            Self::Typst => "main.typ",
            Self::Latex => "main.tex",
            Self::Markdown => "index.md",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSection {
    pub engine: Engine,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateBinding {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bibliography {
    #[serde(default)]
    pub paths: Vec<String>,
}

impl Bibliography {
    fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

/// A parsed and validated `manang.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub project: ProjectSection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<TemplateBinding>,
    #[serde(default, skip_serializing_if = "Bibliography::is_empty")]
    pub bibliography: Bibliography,
}

const TOP_KEYS: &[&str] = &["project", "template", "bibliography"];
const PROJECT_KEYS: &[&str] = &["engine", "name", "main"];
const TEMPLATE_KEYS: &[&str] = &["id", "version"];
const BIBLIOGRAPHY_KEYS: &[&str] = &["paths"];

const BIB_EXTENSIONS: &[&str] = &["bib", "bibtex", "yml", "yaml"];
const MAX_NAME_CHARS: usize = 200;

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn suggestion(key: &str, known: &[&str]) -> Option<String> {
    known
        .iter()
        .map(|k| (edit_distance(&key.to_lowercase(), k), *k))
        .filter(|(d, k)| *d <= 2.max(k.len() / 3))
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k.to_owned())
}

fn check_table(prefix: &str, table: &toml::Table, known: &[&str], issues: &mut Vec<Issue>) {
    for key in table.keys() {
        if known.contains(&key.as_str()) {
            continue;
        }
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        let hint = suggestion(key, known)
            .map(|s| format!("; did you mean `{s}`?"))
            .unwrap_or_default();
        let section = if prefix.is_empty() {
            "the top level".to_owned()
        } else {
            format!("[{prefix}]")
        };
        issues.push(Issue::new(
            full,
            format!(
                "unknown key{hint} (valid keys in {section}: {})",
                known.join(", ")
            ),
        ));
    }
}

fn unknown_keys(table: &toml::Table) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_table("", table, TOP_KEYS, &mut issues);
    for (section, known) in [
        ("project", PROJECT_KEYS),
        ("template", TEMPLATE_KEYS),
        ("bibliography", BIBLIOGRAPHY_KEYS),
    ] {
        if let Some(toml::Value::Table(t)) = table.get(section) {
            check_table(section, t, known, &mut issues);
        }
    }
    issues
}

fn check_relative(key: &str, value: &str, issues: &mut Vec<Issue>) {
    if value.trim().is_empty() {
        issues.push(Issue::new(key, "must not be empty"));
    } else if let Err(e) = lexical_relative(value) {
        issues.push(Issue::new(key, e.to_string()));
    } else if lexical_relative(value).is_ok_and(|p| p.as_os_str().is_empty()) {
        issues.push(Issue::new(key, "must name a file, not the project root"));
    }
}

fn extension_of(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
}

impl ProjectConfig {
    /// A minimal valid config for `engine`.
    pub fn new(engine: Engine, name: Option<String>) -> Self {
        Self {
            project: ProjectSection {
                engine,
                name,
                main: None,
            },
            template: None,
            bibliography: Bibliography::default(),
        }
    }

    /// Parse and validate `manang.toml` source.
    pub fn parse(source: &str) -> Result<Self, ConfigError> {
        let table: toml::Table =
            toml::from_str(source).map_err(|e| ConfigError::Syntax(e.to_string()))?;
        let unknown = unknown_keys(&table);
        if !unknown.is_empty() {
            return Err(ConfigError::Invalid(unknown));
        }
        let config: Self =
            toml::from_str(source).map_err(|e| ConfigError::Schema(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// The root document, applying the engine default.
    pub fn main_file(&self) -> &str {
        self.project
            .main
            .as_deref()
            .unwrap_or_else(|| self.project.engine.default_main())
    }

    /// Check semantic constraints, collecting every problem.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut issues = Vec::new();
        let engine = self.project.engine;

        if let Some(name) = &self.project.name {
            if name.trim().is_empty() {
                issues.push(Issue::new("project.name", "must not be empty"));
            } else if name.chars().count() > MAX_NAME_CHARS {
                issues.push(Issue::new(
                    "project.name",
                    format!("must be at most {MAX_NAME_CHARS} characters"),
                ));
            } else if name.chars().any(char::is_control) {
                issues.push(Issue::new(
                    "project.name",
                    "must not contain control characters",
                ));
            }
        }

        if let Some(main) = &self.project.main {
            let before = issues.len();
            check_relative("project.main", main, &mut issues);
            if issues.len() == before {
                let ok = extension_of(main)
                    .is_some_and(|ext| engine.extensions().contains(&ext.as_str()));
                if !ok {
                    issues.push(Issue::new(
                        "project.main",
                        format!(
                            "`{main}` does not look like a {} document; expected extension {}",
                            engine.as_str(),
                            engine
                                .extensions()
                                .iter()
                                .map(|e| format!("`.{e}`"))
                                .collect::<Vec<_>>()
                                .join(" or ")
                        ),
                    ));
                }
            }
        }

        if let Some(template) = &self.template {
            if template.id.trim().is_empty() {
                issues.push(Issue::new("template.id", "must not be empty"));
            } else if !template
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
            {
                issues.push(Issue::new(
                    "template.id",
                    "may only contain letters, digits, `-`, `_`, `.` and `/`",
                ));
            }
            if template
                .version
                .as_deref()
                .is_some_and(|v| v.trim().is_empty())
            {
                issues.push(Issue::new("template.version", "must not be empty"));
            }
        }

        let mut seen = std::collections::HashSet::new();
        for (i, path) in self.bibliography.paths.iter().enumerate() {
            let key = format!("bibliography.paths[{i}]");
            let before = issues.len();
            check_relative(&key, path, &mut issues);
            if issues.len() != before {
                continue;
            }
            let ok = extension_of(path).is_some_and(|e| BIB_EXTENSIONS.contains(&e.as_str()));
            if !ok {
                issues.push(Issue::new(
                    key,
                    format!(
                        "`{path}` is not a bibliography file; expected one of {}",
                        BIB_EXTENSIONS
                            .iter()
                            .map(|e| format!("`.{e}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            } else if !seen.insert(path.as_str()) {
                issues.push(Issue::new(
                    key,
                    format!("`{path}` is listed more than once"),
                ));
            }
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Invalid(issues))
        }
    }

    /// Serialise to TOML (human-readable, stable key order).
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(|e| ConfigError::Schema(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(source: &str) -> String {
        match ProjectConfig::parse(source) {
            Ok(c) => format!("unexpectedly valid: {c:?}"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn minimal_config_gets_defaults() {
        let c = ProjectConfig::parse("[project]\nengine = \"latex\"\n").ok();
        assert_eq!(c.as_ref().map(ProjectConfig::main_file), Some("main.tex"));
    }

    #[test]
    fn full_config_round_trips() {
        let src = r#"
[project]
engine = "typst"
name = "Thesis"
main = "src/thesis.typ"

[template]
id = "ieee-conference"
version = "1.2.0"

[bibliography]
paths = ["refs/a.bib", "b.yml"]
"#;
        let parsed = ProjectConfig::parse(src).ok();
        let text = parsed.as_ref().and_then(|c| c.to_toml().ok());
        let again = text.as_deref().and_then(|t| ProjectConfig::parse(t).ok());
        assert!(parsed.is_some());
        assert_eq!(parsed, again);
    }

    #[test]
    fn snapshot_syntax_error() {
        insta::assert_snapshot!(err("[project\nengine = 1"));
    }

    #[test]
    fn snapshot_missing_engine() {
        insta::assert_snapshot!(err("[project]\nname = \"x\"\n"));
    }

    #[test]
    fn snapshot_unknown_engine() {
        insta::assert_snapshot!(err("[project]\nengine = \"word\"\n"));
    }

    #[test]
    fn snapshot_unknown_keys_with_suggestions() {
        insta::assert_snapshot!(err(
            "[project]\nengine = \"typst\"\nmian = \"main.typ\"\n\n[bibliograpy]\npaths = []\n\n[template]\nid = \"a\"\nversoin = \"1\"\n"
        ));
    }

    #[test]
    fn snapshot_unknown_key_without_suggestion() {
        insta::assert_snapshot!(err("[project]\nengine = \"typst\"\nzzzzzzzz = 1\n"));
    }

    #[test]
    fn snapshot_every_validation_issue_at_once() {
        insta::assert_snapshot!(err(r#"
[project]
engine = "typst"
name = "  "
main = "../outside.tex"

[template]
id = "bad id!"
version = ""

[bibliography]
paths = ["refs.bib", "refs.bib", "/etc/passwd", "notes.txt", ""]
"#));
    }

    #[test]
    fn snapshot_main_extension_mismatch() {
        insta::assert_snapshot!(err(
            "[project]\nengine = \"markdown\"\nmain = \"main.typ\"\n"
        ));
    }

    #[test]
    fn snapshot_wrong_type() {
        insta::assert_snapshot!(err(
            "[project]\nengine = \"typst\"\n[bibliography]\npaths = \"a.bib\"\n"
        ));
    }
}
