//! Source and preview: map a click on a rendered page to the source position
//! that produced it, and a cursor position in a source file to the places it
//! landed on the pages.
//!
//! Jumps run against the sources of the compile that produced the cached
//! document (a snapshot kept with it), so a later failed compile, whose text
//! no longer matches the pages on screen, cannot skew the answer.
//!
//! Positions on pages are in points from the top-left corner of the page, the
//! coordinate system of the SVG `typst.render` returns. Pages are 0-based like
//! `typst.render`; lines and columns are 1-based, columns counted in
//! characters, like the diagnostics.

use std::collections::HashMap;
use std::num::NonZeroUsize;

use serde::Serialize;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::introspection::PagedPosition;
use typst::layout::{Abs, Point};
use typst::syntax::{FileId, Source, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use typst_ide::{IdeWorld, Jump};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

use crate::diagnostics::file_label;

/// A place on a rendered page.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PagePoint {
    /// 0-based page index, as `typst.render` takes it.
    pub page: usize,
    /// Points from the left edge of the page.
    pub x: f64,
    /// Points from the top edge of the page.
    pub y: f64,
}

/// Where a click on a page leads.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceJump {
    /// A position in a source file.
    File {
        /// Project-relative path (`chapters/intro.typ`), or
        /// `@namespace/name:version/path` for a file inside a package.
        path: String,
        /// 1-based line.
        line: usize,
        /// 1-based column, counted in characters.
        column: usize,
    },
    /// The click hit a link to a web address.
    Url { url: String },
    /// The click hit a link to another place in the document.
    Position(PagePoint),
}

/// Project-relative path for a project file, the diagnostic label otherwise.
fn jump_path(id: FileId) -> String {
    match id.root() {
        VirtualRoot::Project => id.vpath().get_without_slash().to_owned(),
        VirtualRoot::Package(_) => file_label(id),
    }
}

fn page_point(position: &PagedPosition) -> PagePoint {
    PagePoint {
        page: position.page.get().saturating_sub(1),
        x: position.point.x.to_pt(),
        y: position.point.y.to_pt(),
    }
}

/// 1-based line and character column of a byte offset.
fn line_column(source: &Source, byte: usize) -> Option<(usize, usize)> {
    let (line, column) = source.lines().byte_to_line_column(byte)?;
    Some((line + 1, column + 1))
}

/// Byte offset of a 1-based line and character column, clamped into the file:
/// a line past the end means the last line, a column past the end of its line
/// means the end of that line.
pub(crate) fn byte_offset(source: &Source, line: usize, column: usize) -> usize {
    let lines = source.lines();
    let count = lines.len_lines().max(1);
    let index = line.saturating_sub(1).min(count - 1);
    let Some(range) = lines.line_to_range(index) else {
        return source.text().len();
    };
    let text = source.text().get(range.clone()).unwrap_or_default();
    let content = text.trim_end_matches(['\n', '\r']);
    let skip = column.saturating_sub(1);
    let within = content
        .char_indices()
        .nth(skip)
        .map_or(content.len(), |(at, _)| at);
    range.start + within
}

/// The sources one compile read, enough to resolve the spans in its document.
pub(crate) struct SourceSnapshot {
    pub(crate) main: FileId,
    pub(crate) sources: HashMap<FileId, Source>,
}

/// A read-only `World` over a snapshot, for `typst-ide`.
struct SnapshotWorld<'a> {
    library: &'a LazyHash<Library>,
    fonts: &'a FontStore,
    snapshot: &'a SourceSnapshot,
}

impl World for SnapshotWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.snapshot.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.snapshot.sources.get(&id).cloned().ok_or_else(|| {
            FileError::NotFound(std::path::PathBuf::from(id.vpath().get_without_slash()))
        })
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.source(id)
            .map(|source| Bytes::from_string(source.text().to_owned()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

impl IdeWorld for SnapshotWorld<'_> {
    fn upcast(&self) -> &dyn World {
        self
    }
}

/// Resolve a click at `(x, y)` points on the 0-based `page`. The caller has
/// checked that the page exists.
pub(crate) fn from_click(
    library: &LazyHash<Library>,
    fonts: &FontStore,
    snapshot: &SourceSnapshot,
    document: &PagedDocument,
    page: NonZeroUsize,
    x: f64,
    y: f64,
) -> Option<SourceJump> {
    let world = SnapshotWorld {
        library,
        fonts,
        snapshot,
    };
    let position = PagedPosition {
        page,
        point: Point::new(Abs::pt(x), Abs::pt(y)),
    };
    match typst_ide::jump_from_click(&world, document, &position)? {
        Jump::File(id, byte) => {
            let source = snapshot.sources.get(&id)?;
            let (line, column) = line_column(source, byte)?;
            Some(SourceJump::File {
                path: jump_path(id),
                line,
                column,
            })
        }
        Jump::Url(url) => Some(SourceJump::Url {
            url: url.as_str().to_owned(),
        }),
        Jump::Position(position) => Some(SourceJump::Position(page_point(&position))),
    }
}

/// Where the text at a cursor in `source` landed on the pages. Empty when the
/// cursor is not on text that reached the output (markup, code, comments).
pub(crate) fn from_cursor(
    document: &PagedDocument,
    source: &Source,
    line: usize,
    column: usize,
) -> Vec<PagePoint> {
    let cursor = byte_offset(source, line, column);
    typst_ide::jump_from_cursor(document, source, cursor)
        .iter()
        .map(page_point)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(text: &str) -> Source {
        Source::detached(text)
    }

    #[test]
    fn byte_offsets_clamp_into_the_file() {
        let s = source("ab\ncdé\n\nlast");
        assert_eq!(byte_offset(&s, 1, 1), 0);
        assert_eq!(byte_offset(&s, 1, 2), 1);
        // Past the end of the line: the end of that line, not the next one.
        assert_eq!(byte_offset(&s, 1, 40), 2);
        assert_eq!(byte_offset(&s, 2, 3), 5);
        // `é` is two bytes; the column after it is the line's end.
        assert_eq!(byte_offset(&s, 2, 4), 7);
        assert_eq!(byte_offset(&s, 3, 1), 8);
        // Past the last line: the last line.
        assert_eq!(byte_offset(&s, 99, 1), 9);
        // Zero is treated as one, never a panic.
        assert_eq!(byte_offset(&s, 0, 0), 0);
        assert_eq!(byte_offset(&source(""), 1, 1), 0);
    }

    #[test]
    fn line_columns_are_one_based_characters() {
        let s = source("ab\ncdé x");
        assert_eq!(line_column(&s, 0), Some((1, 1)));
        assert_eq!(line_column(&s, 3), Some((2, 1)));
        // After `é` (bytes 5..7) the column counts characters.
        assert_eq!(line_column(&s, 7), Some((2, 4)));
        assert_eq!(line_column(&s, 999), None);
    }

    #[test]
    fn jumps_serialise_as_tagged_variants() {
        let file = SourceJump::File {
            path: "main.typ".to_owned(),
            line: 2,
            column: 1,
        };
        let json = serde_json::to_value(&file).unwrap_or_default();
        assert_eq!(json["kind"], "file");
        assert_eq!(json["path"], "main.typ");
        let at = SourceJump::Position(PagePoint {
            page: 1,
            x: 2.0,
            y: 3.0,
        });
        let json = serde_json::to_value(&at).unwrap_or_default();
        assert_eq!(json["kind"], "position");
        assert_eq!(json["page"], 1);
        let url = SourceJump::Url {
            url: "https://typst.app".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&url).unwrap_or_default()["kind"],
            "url"
        );
    }
}
