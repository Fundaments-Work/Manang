//! The compile session: incremental compilation plus a cache of its result.
//!
//! One [`Compiler`] owns the long-lived stores (fonts, file slots, `comemo`'s
//! global memo tables). Recompiling after a keystroke reuses parsed sources in
//! place and every memoised Typst computation whose inputs did not change.
//! The newest successful [`typst::layout::PagedDocument`] is kept so `render`
//! and `export_pdf` never recompile.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::{Mutex, RwLock};
use rayon::{ThreadPool, ThreadPoolBuilder};
use serde::Serialize;
use typst::Library;
use typst::syntax::FileId;
use typst::utils::LazyHash;
use typst_kit::files::FileStore;
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;
use typst_svg::SvgOptions;

use crate::diagnostics::{self, Diagnostic};
use crate::error::TypstError;
use crate::jump::{self, PagePoint, SourceJump, SourceSnapshot};
use crate::world::{
    Environment, Loader, ProjectWorld, default_library, font_store, package_roots, project_file_id,
};

/// Most unsaved files accepted in one request. Each distinct path is interned
/// by Typst for the life of the process (a 16-bit id space), so this is bounded.
pub const MAX_SOURCES: usize = 1024;

/// How many `comemo` eviction rounds an unused memo survives.
const MEMO_MAX_AGE: usize = 10;

/// Compile threads need headroom: Typst evaluation and layout recurse deeply.
const WORKER_STACK: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub struct CompilerConfig {
    pub env: Environment,
    /// Default project root when a request does not name one.
    pub root: Option<PathBuf>,
    /// Compile threads; `0` lets Rayon choose.
    pub threads: usize,
}

/// One compile request.
#[derive(Debug, Clone, Default)]
pub struct CompileRequest {
    /// Entry file, absolute (inside the root) or relative to the root.
    pub main: String,
    /// Project root. Defaults to the configured root, else the parent of an
    /// absolute `main`.
    pub root: Option<PathBuf>,
    /// Unsaved text keyed by path (absolute inside the root, or relative to
    /// it). Takes precedence over the file on disk.
    pub sources: BTreeMap<String, String>,
}

/// What a compile produced, minus the document itself.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompileReport {
    /// False when Typst reported errors; no document was produced.
    pub ok: bool,
    pub pages: usize,
    /// `[width, height]` of each page in points.
    pub page_sizes: Vec<[f64; 2]>,
    pub diagnostics: Vec<Diagnostic>,
    pub duration_ms: f64,
    /// Increments on every successful compile; render/export use the newest.
    pub generation: u64,
}

/// A compiled document and the artefacts derived from it so far.
struct Artifact {
    generation: u64,
    document: PagedDocument,
    /// The sources this document was compiled from, for jumps.
    sources: SourceSnapshot,
    /// Project root of the compile, to resolve paths in jump requests.
    root: PathBuf,
    svgs: Mutex<BTreeMap<usize, Arc<[u8]>>>,
    pdf: Mutex<Option<Arc<[u8]>>>,
}

struct Session {
    files: FileStore<Loader>,
    generation: u64,
}

pub struct Compiler {
    config: CompilerConfig,
    pool: ThreadPool,
    library: LazyHash<Library>,
    fonts: FontStore,
    /// Embedded fonts only: formulas must not depend on the machine.
    math_fonts: FontStore,
    session: Mutex<Option<Session>>,
    latest: RwLock<Option<Arc<Artifact>>>,
    diagnostics: RwLock<Vec<Diagnostic>>,
}

impl Compiler {
    pub fn new(config: CompilerConfig) -> Result<Self, TypstError> {
        let pool = ThreadPoolBuilder::new()
            .num_threads(config.threads)
            .stack_size(WORKER_STACK)
            .thread_name(|i| format!("manang-typst-{i}"))
            .build()
            .map_err(|e| TypstError::Pool(e.to_string()))?;
        Ok(Self {
            fonts: font_store(&config.env),
            math_fonts: embedded_fonts(),
            library: default_library(),
            pool,
            config,
            session: Mutex::new(None),
            latest: RwLock::new(None),
            diagnostics: RwLock::new(Vec::new()),
        })
    }

    /// Diagnostics of the most recent compile (successful or not).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics.read().clone()
    }

    fn resolve_root(&self, req: &CompileRequest) -> Result<PathBuf, TypstError> {
        if let Some(root) = req.root.as_ref().or(self.config.root.as_ref()) {
            return Ok(root.clone());
        }
        let main = Path::new(&req.main);
        if main.is_absolute() {
            return main
                .parent()
                .map(Path::to_path_buf)
                .ok_or(TypstError::NoRoot);
        }
        Err(TypstError::NoRoot)
    }

    /// Compile `req` on the Rayon pool and cache the result.
    pub fn compile(&self, req: &CompileRequest) -> Result<CompileReport, TypstError> {
        if req.sources.len() > MAX_SOURCES {
            return Err(TypstError::TooManySources(req.sources.len(), MAX_SOURCES));
        }
        let root = self.resolve_root(req)?;
        let main = project_file_id(&root, &req.main)?;
        let mut overrides = std::collections::HashMap::with_capacity(req.sources.len());
        for (path, text) in &req.sources {
            overrides.insert(
                project_file_id(&root, path)?,
                Arc::<str>::from(text.as_str()),
            );
        }

        let mut guard = self.session.lock();
        // A different root invalidates every slot: ids are root-relative.
        let reusable = guard
            .as_ref()
            .is_some_and(|s| s.files.loader().root.path() == root);
        if !reusable {
            let generation = guard.as_ref().map_or(0, |s| s.generation);
            *guard = Some(Session {
                files: FileStore::new(Loader::new(root.clone(), package_roots(&self.config.env))),
                generation,
            });
        }
        let Some(session) = guard.as_mut() else {
            return Err(TypstError::WorkerPanicked);
        };
        session.files.loader_mut().overrides = overrides;
        session.files.reset();

        let started = Instant::now();
        let files = &session.files;
        let outcome = self.pool.install(|| {
            catch_unwind(AssertUnwindSafe(|| {
                let world = ProjectWorld {
                    library: &self.library,
                    fonts: &self.fonts,
                    files,
                    main,
                };
                let warned = typst::compile::<PagedDocument>(&world);
                let result = match &warned.output {
                    Ok(_) => Ok(diagnostics::collect(&world, [], warned.warnings.iter())),
                    Err(errors) => Err(diagnostics::collect(
                        &world,
                        errors.iter(),
                        warned.warnings.iter(),
                    )),
                };
                let document = warned.output.ok();
                // Bound the memo tables; entries untouched for a while go.
                typst::comemo::evict(MEMO_MAX_AGE);
                (document, result)
            }))
        });
        let (document, result) = outcome.map_err(|_| TypstError::WorkerPanicked)?;
        let duration_ms = started.elapsed().as_secs_f64() * 1000.0;

        let (ok, diagnostics) = match result {
            Ok(d) => (true, d),
            Err(d) => (false, d),
        };
        *self.diagnostics.write() = diagnostics.clone();

        let sources = if document.is_some() {
            snapshot_sources(&mut session.files, main)
        } else {
            SourceSnapshot {
                main,
                sources: std::collections::HashMap::new(),
            }
        };

        let mut report = CompileReport {
            ok,
            pages: 0,
            page_sizes: Vec::new(),
            diagnostics,
            duration_ms,
            generation: session.generation,
        };
        if let Some(document) = document {
            session.generation += 1;
            report.generation = session.generation;
            report.pages = document.pages().len();
            report.page_sizes = document
                .pages()
                .iter()
                .map(|p| {
                    let size = p.frame.size();
                    [size.x.to_pt(), size.y.to_pt()]
                })
                .collect();
            *self.latest.write() = Some(Arc::new(Artifact {
                generation: session.generation,
                document,
                sources,
                root,
                svgs: Mutex::new(BTreeMap::new()),
                pdf: Mutex::new(None),
            }));
        }
        Ok(report)
    }

    /// Compile `req` on a scratch session and return only its diagnostics. Nothing the preview
    /// shows changes: the cached document, its generation and the last diagnostics stay as they
    /// are. The agent uses this to check a fix before the author accepts it.
    pub fn check(&self, req: &CompileRequest) -> Result<Vec<Diagnostic>, TypstError> {
        if req.sources.len() > MAX_SOURCES {
            return Err(TypstError::TooManySources(req.sources.len(), MAX_SOURCES));
        }
        let root = self.resolve_root(req)?;
        let main = project_file_id(&root, &req.main)?;
        let mut overrides = std::collections::HashMap::with_capacity(req.sources.len());
        for (path, text) in &req.sources {
            overrides.insert(
                project_file_id(&root, path)?,
                Arc::<str>::from(text.as_str()),
            );
        }
        let mut files = FileStore::new(Loader::new(root, package_roots(&self.config.env)));
        files.loader_mut().overrides = overrides;
        let files = &files;
        let outcome = self.pool.install(|| {
            catch_unwind(AssertUnwindSafe(|| {
                let world = ProjectWorld {
                    library: &self.library,
                    fonts: &self.fonts,
                    files,
                    main,
                };
                let warned = typst::compile::<PagedDocument>(&world);
                let diagnostics = match &warned.output {
                    Ok(_) => diagnostics::collect(&world, [], warned.warnings.iter()),
                    Err(errors) => {
                        diagnostics::collect(&world, errors.iter(), warned.warnings.iter())
                    }
                };
                typst::comemo::evict(MEMO_MAX_AGE);
                diagnostics
            }))
        });
        outcome.map_err(|_| TypstError::WorkerPanicked)
    }

    fn latest(&self) -> Result<Arc<Artifact>, TypstError> {
        self.latest.read().clone().ok_or(TypstError::NoDocument)
    }

    /// Generation of the cached document, if any.
    pub fn generation(&self) -> Option<u64> {
        self.latest.read().as_ref().map(|a| a.generation)
    }

    /// SVG of the 0-based `page` of the newest successfully compiled document.
    pub fn render(&self, page: usize) -> Result<Arc<[u8]>, TypstError> {
        let artifact = self.latest()?;
        if let Some(hit) = artifact.svgs.lock().get(&page) {
            return Ok(Arc::clone(hit));
        }
        let pages = artifact.document.pages();
        let Some(target) = pages.get(page) else {
            return Err(TypstError::PageOutOfRange {
                page,
                pages: pages.len(),
            });
        };
        let svg = self
            .pool
            .install(|| typst_svg::svg(target, &SvgOptions::default()));
        let bytes: Arc<[u8]> = Arc::from(svg.into_bytes());
        artifact.svgs.lock().insert(page, Arc::clone(&bytes));
        Ok(bytes)
    }

    /// Where a click at `(x, y)` points on the 0-based `page` of the newest
    /// document leads: a source position, a URL or another place in the
    /// document. `None` when nothing there maps back (margins, blank space).
    pub fn jump_from_click(
        &self,
        page: usize,
        x: f64,
        y: f64,
    ) -> Result<Option<SourceJump>, TypstError> {
        let artifact = self.latest()?;
        let pages = artifact.document.pages().len();
        if page >= pages {
            return Err(TypstError::PageOutOfRange { page, pages });
        }
        let Some(number) = std::num::NonZeroUsize::new(page + 1) else {
            return Err(TypstError::PageOutOfRange { page, pages });
        };
        if !x.is_finite() || !y.is_finite() {
            return Ok(None);
        }
        let jumped = self.pool.install(|| {
            catch_unwind(AssertUnwindSafe(|| {
                jump::from_click(
                    &self.library,
                    &self.fonts,
                    &artifact.sources,
                    &artifact.document,
                    number,
                    x,
                    y,
                )
            }))
        });
        jumped.map_err(|_| TypstError::WorkerPanicked)
    }

    /// Where the text at the 1-based `line` and character `column` of `path`
    /// (absolute inside the root, or relative to it) appears in the newest
    /// document. Empty when the file was not part of that compile or the
    /// cursor is not on text that reached the pages.
    pub fn jump_from_cursor(
        &self,
        path: &str,
        line: usize,
        column: usize,
    ) -> Result<Vec<PagePoint>, TypstError> {
        let artifact = self.latest()?;
        let id = project_file_id(&artifact.root, path)?;
        let Some(source) = artifact.sources.sources.get(&id) else {
            return Ok(Vec::new());
        };
        let found = self.pool.install(|| {
            catch_unwind(AssertUnwindSafe(|| {
                jump::from_cursor(&artifact.document, source, line, column)
            }))
        });
        found.map_err(|_| TypstError::WorkerPanicked)
    }

    /// PDF of the newest successfully compiled document.
    pub fn export_pdf(&self) -> Result<Arc<[u8]>, TypstError> {
        let artifact = self.latest()?;
        if let Some(hit) = artifact.pdf.lock().as_ref() {
            return Ok(Arc::clone(hit));
        }
        let options = typst_pdf::PdfOptions::default();
        let bytes = self
            .pool
            .install(|| typst_pdf::pdf(&artifact.document, &options))
            .map_err(|errors| {
                TypstError::Pdf(
                    errors
                        .iter()
                        .map(|e| e.message.to_string())
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            })?;
        let bytes: Arc<[u8]> = Arc::from(bytes);
        *artifact.pdf.lock() = Some(Arc::clone(&bytes));
        Ok(bytes)
    }

    /// One formula (Typst math) as a vector SVG whose page is the formula's
    /// bounding box, `size` scene units tall per em, painted `color`
    /// (`currentColor` or `#rrggbb`). Independent of the project session and
    /// deterministic: embedded fonts only, no clock, no files. A formula that
    /// does not compile is a `[math_error]` with a line and column inside it.
    pub fn math_to_svg(
        &self,
        source: &str,
        size: f64,
        color: &str,
    ) -> Result<crate::math::MathSvg, TypstError> {
        let rendered = self.pool.install(|| {
            catch_unwind(AssertUnwindSafe(|| {
                crate::math::convert(&self.library, &self.math_fonts, source, size, color)
            }))
        });
        rendered.map_err(|_| TypstError::WorkerPanicked)?
    }

    /// A one-page PDF of `svg`, the page sized to the picture. Independent of
    /// the project session: it neither reads files nor replaces the cached
    /// document. Deterministic: the same SVG gives the same bytes.
    pub fn svg_to_pdf(&self, svg: &[u8]) -> Result<Arc<[u8]>, TypstError> {
        let converted = self.pool.install(|| {
            catch_unwind(AssertUnwindSafe(|| {
                crate::svgpdf::convert(&self.library, &self.fonts, svg)
            }))
        });
        let bytes = converted.map_err(|_| TypstError::WorkerPanicked)??;
        Ok(Arc::from(bytes))
    }
}

fn embedded_fonts() -> FontStore {
    let mut store = FontStore::new();
    store.extend(typst_kit::fonts::embedded());
    store
}

/// The Typst sources a compile read, kept with its document. Cheap: a
/// `Source` is reference counted, and the store edits its own copy in place on
/// the next compile, leaving this one untouched.
fn snapshot_sources(files: &mut FileStore<Loader>, main: FileId) -> SourceSnapshot {
    let (_, ids) = files.dependencies();
    let ids: Vec<FileId> = ids
        .filter(|id| id.vpath().extension() == Some("typ"))
        .collect();
    let sources = ids
        .into_iter()
        .filter_map(|id| files.source(id).ok().map(|source| (id, source)))
        .collect();
    SourceSnapshot { main, sources }
}
