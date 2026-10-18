//! Built-in starter templates.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::config::Engine;

/// Metadata describing a project starter template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateManifest {
    pub id: String,
    pub name: String,
    pub engine: Engine,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(alias = "main_file")]
    pub main: String,
}

/// A starter template with its manifest and files.
#[derive(Debug, Clone)]
pub struct Template {
    pub manifest: TemplateManifest,
    pub files: &'static [(&'static str, &'static str)],
}

const TEMPLATES_RAW: &[(&str, &[(&str, &str)])] = &[
    (
        include_str!("../../../templates/blank-typst/template.toml"),
        &[(
            "main.typ",
            include_str!("../../../templates/blank-typst/main.typ"),
        )],
    ),
    (
        include_str!("../../../templates/blank-latex/template.toml"),
        &[(
            "main.tex",
            include_str!("../../../templates/blank-latex/main.tex"),
        )],
    ),
    (
        include_str!("../../../templates/blank-markdown/template.toml"),
        &[(
            "index.md",
            include_str!("../../../templates/blank-markdown/index.md"),
        )],
    ),
    (
        include_str!("../../../templates/article-typst/template.toml"),
        &[
            (
                "main.typ",
                include_str!("../../../templates/article-typst/main.typ"),
            ),
            (
                "references.bib",
                include_str!("../../../templates/article-typst/references.bib"),
            ),
        ],
    ),
    (
        include_str!("../../../templates/article-latex/template.toml"),
        &[
            (
                "main.tex",
                include_str!("../../../templates/article-latex/main.tex"),
            ),
            (
                "references.bib",
                include_str!("../../../templates/article-latex/references.bib"),
            ),
        ],
    ),
    (
        include_str!("../../../templates/thesis-chapter-typst/template.toml"),
        &[
            (
                "main.typ",
                include_str!("../../../templates/thesis-chapter-typst/main.typ"),
            ),
            (
                "references.bib",
                include_str!("../../../templates/thesis-chapter-typst/references.bib"),
            ),
        ],
    ),
    (
        include_str!("../../../templates/letter-typst/template.toml"),
        &[(
            "main.typ",
            include_str!("../../../templates/letter-typst/main.typ"),
        )],
    ),
    (
        include_str!("../../../templates/slides-typst/template.toml"),
        &[(
            "main.typ",
            include_str!("../../../templates/slides-typst/main.typ"),
        )],
    ),
    (
        include_str!("../../../templates/poster-typst/template.toml"),
        &[
            (
                "main.typ",
                include_str!("../../../templates/poster-typst/main.typ"),
            ),
            (
                "references.bib",
                include_str!("../../../templates/poster-typst/references.bib"),
            ),
        ],
    ),
    (
        include_str!("../../../templates/lab-report-latex/template.toml"),
        &[
            (
                "main.tex",
                include_str!("../../../templates/lab-report-latex/main.tex"),
            ),
            (
                "references.bib",
                include_str!("../../../templates/lab-report-latex/references.bib"),
            ),
        ],
    ),
    (
        include_str!("../../../templates/notes-markdown/template.toml"),
        &[(
            "index.md",
            include_str!("../../../templates/notes-markdown/index.md"),
        )],
    ),
];

fn parse_templates() -> Vec<Template> {
    TEMPLATES_RAW
        .iter()
        .map(|(manifest_str, files)| {
            let manifest: TemplateManifest =
                toml::from_str(manifest_str).expect("built-in template.toml must be valid TOML");
            Template { manifest, files }
        })
        .collect()
}

/// The built-in templates.
pub fn builtin_templates() -> &'static [Template] {
    static TEMPLATES: OnceLock<Vec<Template>> = OnceLock::new();
    TEMPLATES.get_or_init(parse_templates)
}

/// Summaries of every built-in template for the UI.
pub fn list_templates() -> Vec<TemplateManifest> {
    builtin_templates()
        .iter()
        .map(|t| t.manifest.clone())
        .collect()
}

/// Lookup a built-in template by its identifier.
pub fn find_template(id: &str) -> Option<&'static Template> {
    builtin_templates().iter().find(|t| t.manifest.id == id)
}
