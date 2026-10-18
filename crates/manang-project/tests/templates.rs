use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use manang_project::scaffold::{self, NewProject};
use manang_project::template::{TemplateManifest, find_template, list_templates};
use manang_project::{CONFIG_FILE, Engine};
use manang_typst::{CompileRequest, Compiler, CompilerConfig, DiagSeverity, Environment};
use tempfile::tempdir;

const REQUIRED_TEMPLATES: &[&str] = &[
    "blank-typst",
    "blank-latex",
    "blank-markdown",
    "article-typst",
    "article-latex",
    "thesis-chapter-typst",
    "letter-typst",
    "slides-typst",
    "poster-typst",
    "lab-report-latex",
    "notes-markdown",
];

#[test]
fn every_template_manifest_is_valid() {
    let templates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates");
    assert!(
        templates_dir.is_dir(),
        "templates directory must exist at {}",
        templates_dir.display()
    );

    let mut found_ids = HashSet::new();

    for entry in fs::read_dir(&templates_dir).expect("reading templates directory") {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("valid utf-8 dir name");
        let manifest_path = path.join("template.toml");
        assert!(
            manifest_path.is_file(),
            "missing template.toml in {}",
            path.display()
        );

        let manifest_content = fs::read_to_string(&manifest_path).expect("reading template.toml");
        let manifest: TemplateManifest =
            toml::from_str(&manifest_content).expect("parsing template.toml");

        assert_eq!(
            manifest.id,
            dir_name,
            "template id must match folder name in {}",
            manifest_path.display()
        );
        assert!(
            !manifest.name.trim().is_empty(),
            "template name must not be empty in {}",
            manifest_path.display()
        );
        assert!(
            !manifest.description.trim().is_empty(),
            "template description must not be empty in {}",
            manifest_path.display()
        );
        assert!(
            !manifest.tags.is_empty(),
            "template tags must not be empty in {}",
            manifest_path.display()
        );
        assert!(
            !manifest.main.trim().is_empty(),
            "template main must not be empty in {}",
            manifest_path.display()
        );

        let main_file_path = path.join(&manifest.main);
        assert!(
            main_file_path.is_file(),
            "main file {} must exist in {}",
            manifest.main,
            path.display()
        );

        // If references are cited, references.bib should exist
        let main_content = fs::read_to_string(&main_file_path).unwrap_or_default();
        if main_content.contains("references.bib") || main_content.contains("\\bibliography{") {
            assert!(
                path.join("references.bib").is_file(),
                "template {} references a bib file but references.bib is missing",
                manifest.id
            );
        }

        found_ids.insert(manifest.id);
    }

    for required in REQUIRED_TEMPLATES {
        assert!(
            found_ids.contains(*required),
            "required template `{required}` is missing from templates directory"
        );
    }
}

#[test]
fn built_in_templates_match_required_list() {
    let list = list_templates();
    assert_eq!(list.len(), 11);

    for required in REQUIRED_TEMPLATES {
        let found = list.iter().find(|t| t.id == *required);
        assert!(
            found.is_some(),
            "required template `{required}` not found in list_templates()"
        );
        assert!(find_template(required).is_some());
    }
}

#[test]
fn scaffold_populates_template_files_and_config() {
    let dir = tempdir().expect("tempdir");
    let config = scaffold::create(
        dir.path(),
        &NewProject {
            name: "Quantum Thermodynamics",
            engine: Engine::Typst,
            template: Some("article-typst"),
        },
    )
    .expect("scaffold succeeds");

    assert_eq!(config.project.engine, Engine::Typst);
    assert_eq!(
        config.template.as_ref().map(|t| t.id.as_str()),
        Some("article-typst")
    );
    assert_eq!(config.project.main.as_deref(), Some("main.typ"));
    assert_eq!(config.bibliography.paths, vec!["references.bib"]);

    assert!(dir.path().join(CONFIG_FILE).is_file());
    assert!(dir.path().join("main.typ").is_file());
    assert!(dir.path().join("references.bib").is_file());
    assert!(dir.path().join(".gitignore").is_file());
    assert!(!dir.path().join("template.toml").exists());

    let main_content = fs::read_to_string(dir.path().join("main.typ")).expect("read main.typ");
    assert!(main_content.contains("Quantum Thermodynamics"));
    assert!(!main_content.contains("{{title}}"));
}

#[test]
fn every_typst_template_compiles_through_typst_compiler() {
    let compiler = Compiler::new(CompilerConfig {
        env: Environment::default(),
        root: None,
        threads: 2,
    })
    .expect("typst compiler starts");

    let typst_templates = [
        "blank-typst",
        "article-typst",
        "thesis-chapter-typst",
        "letter-typst",
        "slides-typst",
        "poster-typst",
    ];

    for template_id in typst_templates {
        let dir = tempdir().expect("tempdir");
        let project_name = format!("Test {}", template_id);
        let config = scaffold::create(
            dir.path(),
            &NewProject {
                name: &project_name,
                engine: Engine::Typst,
                template: Some(template_id),
            },
        )
        .unwrap_or_else(|e| panic!("scaffold failed for {template_id}: {e}"));

        let main_file = config
            .project
            .main
            .as_deref()
            .unwrap_or_else(|| Engine::Typst.default_main());

        let report = compiler
            .compile(&CompileRequest {
                main: main_file.to_owned(),
                root: Some(dir.path().to_path_buf()),
                sources: BTreeMap::new(),
            })
            .unwrap_or_else(|e| panic!("compile failed for {template_id}: {e}"));

        let errors: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == DiagSeverity::Error)
            .collect();

        assert!(
            errors.is_empty(),
            "template `{template_id}` had compilation errors: {errors:?}"
        );
    }
}
