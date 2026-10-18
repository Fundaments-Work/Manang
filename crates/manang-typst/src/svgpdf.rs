//! One SVG to a one-page PDF, through Typst itself: a tiny document with an
//! automatic page size and no margin that places the image, exported with
//! `typst-pdf`. LaTeX includes the result with `\includegraphics`, so no
//! Inkscape (and no `svg` package) is needed.
//!
//! The world here has exactly two files, both in memory: the document and
//! the SVG. Their ids are fixed paths, so repeated conversions intern nothing
//! new in Typst's process-wide id table.

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

use crate::error::TypstError;

/// Largest SVG accepted, matching the project plugin's read limit.
pub const MAX_SVG_BYTES: usize = 64 * 1024 * 1024;

const MAIN_PATH: &str = "/manang-svg-to-pdf/main.typ";
const SVG_PATH: &str = "/manang-svg-to-pdf/picture.svg";

/// The page fits the picture exactly.
const DOCUMENT: &str = "#set page(width: auto, height: auto, margin: 0pt)\n\
#image(\"picture.svg\")\n";

fn file_id(path: &str) -> Result<FileId, TypstError> {
    let vpath = VirtualPath::new(path).map_err(|e| TypstError::BadPath {
        path: path.to_owned(),
        reason: e.to_string(),
    })?;
    Ok(RootedPath::new(VirtualRoot::Project, vpath).intern())
}

struct SvgWorld<'a> {
    library: &'a LazyHash<Library>,
    fonts: &'a FontStore,
    main: Source,
    svg_id: FileId,
    svg: Bytes,
}

impl World for SvgWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotSource)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.svg_id {
            Ok(self.svg.clone())
        } else if id == self.main.id() {
            Ok(Bytes::from_string(self.main.text().to_owned()))
        } else {
            Err(FileError::AccessDenied)
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    /// No clock: the PDF must be byte-identical for the same SVG.
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Compile the one-image document for `svg` and export it as PDF. Runs on
/// the caller's thread; [`crate::Compiler::svg_to_pdf`] moves it to the pool.
pub(crate) fn convert(
    library: &LazyHash<Library>,
    fonts: &FontStore,
    svg: &[u8],
) -> Result<Vec<u8>, TypstError> {
    if svg.len() > MAX_SVG_BYTES {
        return Err(TypstError::Svg(format!(
            "the SVG is {} bytes; the limit is {MAX_SVG_BYTES}",
            svg.len()
        )));
    }
    if std::str::from_utf8(svg).is_err() {
        return Err(TypstError::Svg("the SVG is not UTF-8 text".to_owned()));
    }
    let world = SvgWorld {
        library,
        fonts,
        main: Source::new(file_id(MAIN_PATH)?, DOCUMENT.to_owned()),
        svg_id: file_id(SVG_PATH)?,
        svg: Bytes::new(svg.to_vec()),
    };
    let document = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| TypstError::Svg(join(errors.iter().map(|e| e.message.as_str()))))?;
    typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|errors| TypstError::Pdf(join(errors.iter().map(|e| e.message.as_str()))))
}

fn join<'a>(messages: impl Iterator<Item = &'a str>) -> String {
    messages.collect::<Vec<_>>().join("; ")
}
