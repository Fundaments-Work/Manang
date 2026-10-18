//! Rendered figures next to their sources, written by the project plugin.
//!
//! The diagram plugin is pure (no filesystem, ADR 0011) and the Typst plugin
//! only reads, so the files a document includes are written here, where every
//! other project write happens: confined to the root, atomically. The
//! rendering itself is delegated through the kernel ([`Context::invoke`]):
//!
//! * `diagram.languages`, `diagram.key` and `diagram.render` (theme `print`)
//!   for the SVG, and
//! * `typst.svg-to-pdf` for the PDF that LaTeX includes.
//!
//! An SVG written here starts with `<!-- manang-diagram <language> key=<key>
//! -->`. When the file on disk already carries the key of the current source
//! it is left alone, so saving an unchanged diagram never touches git.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use manang_kernel::{Context, KernelError, Payload};
use serde::Serialize;
use serde_json::{Value, json};

use crate::config::Engine;
use crate::error::ProjectError;
use crate::fsops;
use crate::paths::{ProjectRoot, STATE_DIR, to_slash};

/// Theme of every committed or cached file: print-ready on white paper. The
/// app theme only changes the on-screen preview.
pub const EXPORT_THEME: &str = "print";

/// Where inline diagrams (Markdown fences) are rendered, project-relative.
pub const DIAGRAM_CACHE_DIR: &str = ".manang/cache/diagrams";

/// Keeps git away from everything under `.manang/cache/`, even in projects
/// whose `.gitignore` does not list `.manang/` (projects not created by
/// Manang). Git honours a `.gitignore` in any directory; `*` ignores the
/// directory's contents, this file included.
const CACHE_GITIGNORE: &str = "# Written by Manang: rendered caches, safe to delete.\n*\n";

/// What one export did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Exported {
    /// False when the source has an error; the previous files are untouched.
    pub ok: bool,
    /// The rendered SVG, project-relative (also when it was already current).
    pub svg: Option<String>,
    /// The rendered PDF, when the project includes PDFs (LaTeX).
    pub pdf: Option<String>,
    /// The files this call wrote; empty when everything was current.
    pub written: Vec<String>,
    /// The diagram plugin's error for the source (`[diagram_error] line L:C:
    /// ...`), when `ok` is false.
    pub error: Option<String>,
}

/// The first line of every SVG this module writes.
pub fn header(language: &str, key: &str) -> String {
    format!("<!-- manang-diagram {language} key={key} -->\n")
}

/// Whether `existing` (the start of an SVG file) was written for `key`.
pub fn carries_key(existing: &[u8], language: &str, key: &str) -> bool {
    existing.starts_with(header(language, key).as_bytes())
}

/// `dir/name.ext` with a new extension, `/`-separated.
pub fn sibling(path: &str, extension: &str) -> String {
    let (dir, name) = match path.rfind('/') {
        Some(at) => (&path[..=at], &path[at + 1..]),
        None => ("", path),
    };
    let stem = match name.rfind('.') {
        Some(at) if at > 0 => &name[..at],
        _ => name,
    };
    format!("{dir}{stem}.{extension}")
}

fn delegated(error: KernelError) -> ProjectError {
    ProjectError::Delegate(error.to_string())
}

fn json(payload: Payload) -> Result<Value, ProjectError> {
    match payload {
        Payload::Json(value) => Ok(value),
        _ => Err(ProjectError::Delegate(
            "a plugin answered with something other than JSON".to_owned(),
        )),
    }
}

fn binary(payload: Payload) -> Result<Arc<[u8]>, ProjectError> {
    match payload {
        Payload::Binary(bytes) => Ok(bytes),
        _ => Err(ProjectError::Delegate(
            "a plugin answered with something other than bytes".to_owned(),
        )),
    }
}

/// The language whose extensions (from `diagram.languages`) include
/// `path`'s, or None.
fn language_of(ctx: &Context<'_>, path: &str) -> Result<Option<String>, ProjectError> {
    let Some(extension) = Path::new(path).extension().and_then(|e| e.to_str()) else {
        return Ok(None);
    };
    let dotted = format!(".{}", extension.to_ascii_lowercase());
    let listed = json(
        ctx.invoke("diagram.languages", Payload::Empty)
            .map_err(delegated)?,
    )?;
    let languages = listed
        .get("languages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    Ok(languages.iter().find_map(|language| {
        let matches = language
            .get("extensions")
            .and_then(Value::as_array)
            .is_some_and(|exts| exts.iter().any(|e| e.as_str() == Some(dotted.as_str())));
        matches
            .then(|| {
                language
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .flatten()
    }))
}

fn key(ctx: &Context<'_>, language: &str, source: &str) -> Result<String, ProjectError> {
    let out = json(
        ctx.invoke(
            "diagram.key",
            Payload::Json(json!({ "language": language, "source": source, "theme": EXPORT_THEME })),
        )
        .map_err(delegated)?,
    )?;
    out.get("key")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ProjectError::Delegate("`diagram.key` returned no key".to_owned()))
}

/// The SVG for `source`, or the diagram plugin's message when the source
/// cannot be drawn. Failures of the kernel itself are errors.
fn render(
    ctx: &Context<'_>,
    language: &str,
    source: &str,
) -> Result<Result<Arc<[u8]>, String>, ProjectError> {
    match ctx.invoke(
        "diagram.render",
        Payload::Json(json!({ "language": language, "source": source, "theme": EXPORT_THEME })),
    ) {
        Ok(payload) => binary(payload).map(Ok),
        Err(KernelError::Plugin(error)) => Ok(Err(error.to_string())),
        Err(other) => Err(delegated(other)),
    }
}

fn svg_to_pdf(ctx: &Context<'_>, svg: &[u8]) -> Result<Arc<[u8]>, ProjectError> {
    binary(
        ctx.invoke("typst.svg-to-pdf", Payload::Binary(Arc::from(svg)))
            .map_err(delegated)?,
    )
}

/// The first bytes of a file, or nothing when it does not exist.
fn head(path: &Path, len: usize) -> Vec<u8> {
    use std::io::Read;
    let mut buf = Vec::with_capacity(len);
    if let Ok(file) = fs::File::open(path) {
        let _ = file.take(len as u64).read_to_end(&mut buf);
    }
    buf
}

/// True when `pdf` exists and is not older than `svg`.
fn pdf_current(pdf: &Path, svg: &Path) -> bool {
    let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    match (modified(pdf), modified(svg)) {
        (Some(pdf), Some(svg)) => pdf >= svg,
        _ => false,
    }
}

/// Whether the project includes figures as PDF: a LaTeX project, or any
/// project with a `.tex` file (git-ignored files and `.manang/` excluded).
pub fn wants_pdf(root: &ProjectRoot, engine: Engine) -> bool {
    if engine == Engine::Latex {
        return true;
    }
    let state = root.path().join(STATE_DIR);
    ignore::WalkBuilder::new(root.path())
        .hidden(true)
        .git_global(false)
        .require_git(false)
        .follow_links(false)
        .filter_entry(move |e| e.path() != state)
        .build()
        .filter_map(Result::ok)
        .any(|e| {
            e.file_type().is_some_and(|t| t.is_file())
                && e.path()
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("tex"))
        })
}

fn write(root: &ProjectRoot, rel: &str, bytes: &[u8]) -> Result<(), ProjectError> {
    if bytes.len() > fsops::MAX_WRITE_BYTES {
        return Err(ProjectError::TooLarge {
            path: rel.to_owned(),
            size: bytes.len() as u64,
            limit: fsops::MAX_WRITE_BYTES as u64,
        });
    }
    let path = root.resolve_for_write(rel)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| ProjectError::io(format!("creating directories for {rel}"), e))?;
    }
    fsops::atomic_write(&path, bytes)
}

/// Write `<svg stem>.pdf` next to `svg_rel` unless it is already current.
/// Returns the PDF's path and whether it was written.
fn write_pdf(
    ctx: &Context<'_>,
    root: &ProjectRoot,
    svg_rel: &str,
    svg: Option<&[u8]>,
) -> Result<(String, bool), ProjectError> {
    let pdf_rel = sibling(svg_rel, "pdf");
    let svg_path = root.resolve(svg_rel)?;
    let pdf_path = root.resolve_for_write(&pdf_rel)?;
    if svg.is_none() && pdf_current(&pdf_path, &svg_path) {
        return Ok((pdf_rel, false));
    }
    let bytes = match svg {
        Some(bytes) => bytes.to_vec(),
        None => fsops::read_bytes(root, svg_rel)?,
    };
    let pdf = svg_to_pdf(ctx, &bytes)?;
    write(root, &pdf_rel, &pdf)?;
    Ok((pdf_rel, true))
}

/// `project.export-diagram`: render the diagram file `path` to its sibling
/// `.svg` (and `.pdf` when `pdf` is true), skipping what is current.
pub fn export_diagram(
    ctx: &Context<'_>,
    root: &ProjectRoot,
    path: &str,
    pdf: bool,
) -> Result<Exported, ProjectError> {
    let language = language_of(ctx, path)?.ok_or_else(|| ProjectError::InvalidPath {
        path: path.to_owned(),
        reason: "not a diagram file (.mmd, .mermaid, .pikchr, .bob)",
    })?;
    let source = fsops::read_text(root, path)?;
    let svg_rel = sibling(path, "svg");
    let svg_path = root.resolve_for_write(&svg_rel)?;
    let key = key(ctx, &language, &source)?;
    let mut written = Vec::new();

    let header = header(&language, &key);
    let fresh = if carries_key(&head(&svg_path, header.len()), &language, &key) {
        None
    } else {
        let svg = match render(ctx, &language, &source)? {
            Ok(svg) => svg,
            Err(message) => {
                return Ok(Exported {
                    ok: false,
                    svg: svg_path.is_file().then_some(svg_rel),
                    pdf: None,
                    written,
                    error: Some(message),
                });
            }
        };
        let mut bytes = Vec::with_capacity(header.len() + svg.len());
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend_from_slice(&svg);
        write(root, &svg_rel, &bytes)?;
        written.push(svg_rel.clone());
        Some(bytes)
    };

    let pdf_rel = if pdf {
        let (pdf_rel, wrote) = write_pdf(ctx, root, &svg_rel, fresh.as_deref())?;
        if wrote {
            written.push(pdf_rel.clone());
        }
        Some(pdf_rel)
    } else {
        None
    };
    Ok(Exported {
        ok: true,
        svg: Some(svg_rel),
        pdf: pdf_rel,
        written,
        error: None,
    })
}

/// `project.cache-diagram`: render an inline diagram (a Markdown fence) to
/// `.manang/cache/diagrams/<key>.svg`, unless that file exists. The key
/// names the content, so an existing file is always current.
pub fn cache_diagram(
    ctx: &Context<'_>,
    root: &ProjectRoot,
    language: &str,
    source: &str,
) -> Result<Exported, ProjectError> {
    let key = key(ctx, language, source)?;
    let svg_rel = format!("{DIAGRAM_CACHE_DIR}/{key}.svg");
    let svg_path = root.resolve_for_write(&svg_rel)?;
    if svg_path.is_file() {
        return Ok(Exported {
            ok: true,
            svg: Some(svg_rel),
            pdf: None,
            written: Vec::new(),
            error: None,
        });
    }
    let svg = match render(ctx, language, source)? {
        Ok(svg) => svg,
        Err(message) => {
            return Ok(Exported {
                ok: false,
                svg: None,
                pdf: None,
                written: Vec::new(),
                error: Some(message),
            });
        }
    };
    ensure_cache_ignored(root)?;
    let mut bytes = header(language, &key).into_bytes();
    bytes.extend_from_slice(&svg);
    write(root, &svg_rel, &bytes)?;
    Ok(Exported {
        ok: true,
        svg: Some(svg_rel.clone()),
        pdf: None,
        written: vec![svg_rel],
        error: None,
    })
}

/// Put [`CACHE_GITIGNORE`] in `.manang/cache/` unless a file is there.
fn ensure_cache_ignored(root: &ProjectRoot) -> Result<(), ProjectError> {
    let rel = format!("{STATE_DIR}/cache/.gitignore");
    let path = root.resolve_for_write(&rel)?;
    if path.exists() {
        return Ok(());
    }
    write(root, &rel, CACHE_GITIGNORE.as_bytes())
}

/// `project.export-pdf`: write `<name>.pdf` next to the SVG at `path`
/// (project-relative, or absolute inside the project) when the project
/// includes PDFs, or always with `force`. Returns the PDF's path, or None
/// when the project does not want one.
pub fn export_pdf(
    ctx: &Context<'_>,
    root: &ProjectRoot,
    path: &str,
    wanted: bool,
) -> Result<Exported, ProjectError> {
    let rel = relative_in(root, path)?;
    if !rel.to_ascii_lowercase().ends_with(".svg") {
        return Err(ProjectError::InvalidPath {
            path: path.to_owned(),
            reason: "not an SVG file",
        });
    }
    if !wanted {
        return Ok(Exported {
            ok: true,
            svg: Some(rel),
            pdf: None,
            written: Vec::new(),
            error: None,
        });
    }
    let (pdf_rel, wrote) = write_pdf(ctx, root, &rel, None)?;
    Ok(Exported {
        ok: true,
        svg: Some(rel),
        written: if wrote {
            vec![pdf_rel.clone()]
        } else {
            Vec::new()
        },
        pdf: Some(pdf_rel),
        error: None,
    })
}

/// `path` as a project-relative `/`-separated path. An absolute path must lie
/// inside the root (symlinks resolved); a relative one is checked later by
/// the usual confinement.
pub fn relative_in(root: &ProjectRoot, path: &str) -> Result<String, ProjectError> {
    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return Ok(path.to_owned());
    }
    let parent = candidate
        .parent()
        .ok_or_else(|| ProjectError::Escapes(path.to_owned()))?;
    let name = candidate
        .file_name()
        .ok_or_else(|| ProjectError::Escapes(path.to_owned()))?;
    let parent = parent
        .canonicalize()
        .map_err(|_| ProjectError::NotFound(path.to_owned()))?;
    let rel = root
        .relative(&parent.join(name))
        .ok_or_else(|| ProjectError::Escapes(path.to_owned()))?;
    to_slash(&rel).ok_or(ProjectError::InvalidPath {
        path: path.to_owned(),
        reason: "path is not valid UTF-8",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siblings_swap_the_last_extension() {
        assert_eq!(sibling("figures/flow.mmd", "svg"), "figures/flow.svg");
        assert_eq!(sibling("flow.mermaid", "svg"), "flow.svg");
        assert_eq!(sibling("a.b/c.d.bob", "pdf"), "a.b/c.d.pdf");
        assert_eq!(sibling("figures/x.svg", "pdf"), "figures/x.pdf");
        assert_eq!(sibling(".hidden", "svg"), ".hidden.svg");
    }

    #[test]
    fn the_header_names_language_and_key() {
        let h = header("mermaid", "ab12");
        assert_eq!(h, "<!-- manang-diagram mermaid key=ab12 -->\n");
        assert!(carries_key(
            format!("{h}<svg/>").as_bytes(),
            "mermaid",
            "ab12"
        ));
        assert!(!carries_key(h.as_bytes(), "mermaid", "ab13"));
        assert!(!carries_key(h.as_bytes(), "pikchr", "ab12"));
        assert!(!carries_key(b"<svg/>", "mermaid", "ab12"));
        assert!(!carries_key(b"", "mermaid", "ab12"));
    }

    #[test]
    fn tex_files_make_a_project_want_pdfs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = ProjectRoot::open(dir.path()).expect("root");
        assert!(wants_pdf(&root, Engine::Latex));
        assert!(!wants_pdf(&root, Engine::Typst));
        fs::create_dir_all(dir.path().join(".manang/trash")).expect("dir");
        fs::write(dir.path().join(".manang/trash/old.tex"), "").expect("w");
        assert!(!wants_pdf(&root, Engine::Typst), "the trash does not count");
        fs::create_dir_all(dir.path().join("appendix")).expect("dir");
        fs::write(dir.path().join("appendix/a.TEX"), "").expect("w");
        assert!(wants_pdf(&root, Engine::Markdown));
    }

    #[test]
    fn absolute_paths_must_be_inside_the_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside");
        let root = ProjectRoot::open(dir.path()).expect("root");
        fs::create_dir_all(dir.path().join("figures")).expect("dir");
        let inside = dir.path().join("figures/x.svg");
        assert_eq!(
            relative_in(&root, inside.to_str().expect("utf8")).expect("inside"),
            "figures/x.svg"
        );
        assert_eq!(
            relative_in(&root, "figures/x.svg").expect("relative"),
            "figures/x.svg"
        );
        let away = outside.path().join("x.svg");
        assert!(relative_in(&root, away.to_str().expect("utf8")).is_err());
    }
}
