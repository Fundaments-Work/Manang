//! SyncTeX: map source positions to PDF positions and back.
//!
//! Tectonic writes a gzipped `.synctex.gz`. We parse it ourselves (the format
//! is line-oriented text) so no C parser is needed. All coordinates handed out
//! are PDF points (1/72 in), measured from the top-left of the page.

use std::collections::HashMap;
use std::io::Read;

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};

use crate::error::LatexError;

/// Refuse to inflate more than this much SyncTeX text.
const MAX_INFLATED: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    VBox,
    HBox,
    VoidV,
    VoidH,
    Point,
}

#[derive(Debug, Clone, Copy)]
struct Record {
    page: u32,
    kind: Kind,
    tag: u32,
    line: u32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    depth: f32,
}

impl Record {
    /// Bounding box as `(left, top, right, bottom)`.
    fn rect(&self) -> (f32, f32, f32, f32) {
        match self.kind {
            Kind::HBox | Kind::VoidH => (
                self.x,
                self.y - self.height,
                self.x + self.width,
                self.y + self.depth,
            ),
            Kind::VBox | Kind::VoidV => (
                self.x,
                self.y,
                self.x + self.width,
                self.y + self.height + self.depth,
            ),
            Kind::Point => (self.x, self.y, self.x + self.width, self.y),
        }
    }
}

/// A place in the PDF.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PdfPosition {
    /// 1-based page number.
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A place in the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePosition {
    pub file: String,
    pub line: u32,
}

/// A parsed SyncTeX file.
#[derive(Debug, Clone)]
pub struct Synctex {
    inputs: HashMap<u32, String>,
    records: Vec<Record>,
}

/// TeX scaled points to PDF points, given the file's unit and magnification.
fn to_bp(value: i64, scale: f64) -> f32 {
    (value as f64 * scale / 65536.0 * 72.0 / 72.27) as f32
}

fn split_pair(s: &str, sep: char) -> (&str, &str) {
    s.split_once(sep).unwrap_or((s, ""))
}

fn int(s: &str) -> Option<i64> {
    s.trim().parse().ok()
}

impl Synctex {
    /// Inflate and parse `.synctex.gz` bytes.
    pub fn from_gz(bytes: &[u8]) -> Result<Self, LatexError> {
        let mut text = String::new();
        GzDecoder::new(bytes)
            .take(MAX_INFLATED)
            .read_to_string(&mut text)
            .map_err(|e| LatexError::BadSynctex(e.to_string()))?;
        Self::parse(&text)
    }

    /// Parse plain SyncTeX text. Malformed records are skipped.
    pub fn parse(text: &str) -> Result<Self, LatexError> {
        let mut inputs = HashMap::new();
        let mut records = Vec::new();
        let mut unit = 1.0_f64;
        let mut magnification = 1000.0_f64;
        let mut in_content = false;
        let mut page = 0_u32;

        for line in text.lines() {
            if !in_content {
                if let Some(rest) = line.strip_prefix("Input:") {
                    let (tag, path) = split_pair(rest, ':');
                    if let Some(tag) = int(tag).and_then(|t| u32::try_from(t).ok()) {
                        inputs.insert(tag, path.to_owned());
                    }
                } else if let Some(rest) = line.strip_prefix("Unit:") {
                    unit = int(rest).map_or(1.0, |v| v as f64);
                } else if let Some(rest) = line.strip_prefix("Magnification:") {
                    magnification = int(rest).map_or(1000.0, |v| v as f64);
                } else if line.starts_with("Content:") {
                    in_content = true;
                }
                continue;
            }
            let scale = unit * magnification / 1000.0;
            let mut chars = line.chars();
            let Some(first) = chars.next() else { continue };
            let rest = chars.as_str();
            match first {
                '{' => {
                    page = int(rest)
                        .and_then(|p| u32::try_from(p).ok())
                        .unwrap_or(page);
                }
                '[' | '(' | 'v' | 'h' | 'k' | 'g' | '$' | 'x' => {
                    if let Some(record) = parse_record(first, rest, page, scale) {
                        records.push(record);
                    }
                }
                _ => {}
            }
        }
        if !in_content {
            return Err(LatexError::BadSynctex("no `Content:` section".to_owned()));
        }
        Ok(Self { inputs, records })
    }

    fn tag_for(&self, file: &str, root: &str) -> Option<u32> {
        let wanted = file.trim_start_matches("./");
        self.inputs.iter().find_map(|(tag, path)| {
            let relative = relative_to(path, root);
            (relative == wanted || path == wanted).then_some(*tag)
        })
    }

    /// Where a source line landed in the PDF. `root` is the directory the
    /// build ran in; `file` is relative to it. When the line produced no
    /// output (comments, blank lines) the nearest following line that did is
    /// used.
    pub fn forward(
        &self,
        file: &str,
        line: u32,
        root: &str,
    ) -> Result<Vec<PdfPosition>, LatexError> {
        let tag = self.tag_for(file, root).ok_or_else(|| {
            LatexError::SynctexNoMatch(format!("`{file}` is not part of the build"))
        })?;
        let candidates = self.records.iter().filter(|r| r.tag == tag);
        let best_line = candidates
            .clone()
            .filter(|r| r.line >= line)
            .map(|r| r.line)
            .min()
            .or_else(|| candidates.clone().map(|r| r.line).max());
        let Some(best_line) = best_line else {
            return Err(LatexError::SynctexNoMatch(format!(
                "`{file}` produced no output"
            )));
        };

        // Union of everything on that line, per page, in page order.
        let mut by_page: Vec<(u32, (f32, f32, f32, f32))> = Vec::new();
        for r in candidates.filter(|r| r.line == best_line) {
            let (l, t, rt, b) = r.rect();
            match by_page.iter_mut().find(|(p, _)| *p == r.page) {
                Some((_, u)) => {
                    u.0 = u.0.min(l);
                    u.1 = u.1.min(t);
                    u.2 = u.2.max(rt);
                    u.3 = u.3.max(b);
                }
                None => by_page.push((r.page, (l, t, rt, b))),
            }
        }
        by_page.sort_by_key(|(p, _)| *p);
        Ok(by_page
            .into_iter()
            .map(|(page, (l, t, r, b))| PdfPosition {
                page,
                x: l,
                y: t,
                width: r - l,
                height: b - t,
            })
            .collect())
    }

    /// Which source line produced the content at `(x, y)` on `page`.
    pub fn reverse(
        &self,
        page: u32,
        x: f32,
        y: f32,
        root: &str,
    ) -> Result<SourcePosition, LatexError> {
        let on_page = self.records.iter().filter(|r| r.page == page && r.line > 0);
        // Prefer the smallest horizontal box that contains the point.
        let containing = on_page
            .clone()
            .filter(|r| matches!(r.kind, Kind::HBox | Kind::VoidH))
            .filter(|r| {
                let (l, t, rt, b) = r.rect();
                x >= l && x <= rt && y >= t && y <= b
            })
            .min_by(|a, b| area(a).total_cmp(&area(b)));
        // Inside that box, the glue/kern/math nodes know which file the text
        // came from (a paragraph box is tagged with where it was closed, not
        // where its words were read), so prefer the nearest of those.
        let nearest_child = containing.and_then(|outer| {
            let (l, t, rt, b) = outer.rect();
            on_page
                .clone()
                .filter(|r| {
                    matches!(r.kind, Kind::Point | Kind::VoidH)
                        && r.x >= l
                        && r.x <= rt
                        && r.y >= t
                        && r.y <= b
                })
                .min_by(|p, q| (p.x - x).abs().total_cmp(&(q.x - x).abs()))
        });
        let chosen = nearest_child
            .or(containing)
            .or_else(|| on_page.min_by(|a, b| distance(a, x, y).total_cmp(&distance(b, x, y))));
        let record = chosen.ok_or_else(|| {
            LatexError::SynctexNoMatch(format!("page {page} has no mapped content"))
        })?;
        let path = self
            .inputs
            .get(&record.tag)
            .ok_or_else(|| LatexError::BadSynctex(format!("unknown input tag {}", record.tag)))?;
        Ok(SourcePosition {
            file: relative_to(path, root).to_owned(),
            line: record.line,
        })
    }
}

fn area(r: &Record) -> f32 {
    let (l, t, rt, b) = r.rect();
    (rt - l).max(0.0) * (b - t).max(0.0)
}

fn distance(r: &Record, x: f32, y: f32) -> f32 {
    let (l, t, rt, b) = r.rect();
    let dx = if x < l {
        l - x
    } else if x > rt {
        x - rt
    } else {
        0.0
    };
    let dy = if y < t {
        t - y
    } else if y > b {
        y - b
    } else {
        0.0
    };
    dx * dx + dy * dy
}

fn relative_to<'a>(path: &'a str, root: &str) -> &'a str {
    let root = root.trim_end_matches('/');
    path.strip_prefix(root)
        .map(|p| p.trim_start_matches('/'))
        .unwrap_or_else(|| path.trim_start_matches("./"))
}

fn parse_record(first: char, rest: &str, page: u32, scale: f64) -> Option<Record> {
    let kind = match first {
        '[' => Kind::VBox,
        '(' => Kind::HBox,
        'v' => Kind::VoidV,
        'h' => Kind::VoidH,
        _ => Kind::Point,
    };
    let mut sections = rest.split(':');
    let head = sections.next()?;
    let pos = sections.next()?;
    let size = sections.next().unwrap_or("");

    let mut ids = head.split(',');
    let tag = u32::try_from(int(ids.next()?)?).ok()?;
    let line = u32::try_from(int(ids.next()?)?).ok()?;

    let (x, y) = split_pair(pos, ',');
    let mut dims = size.split(',');
    let mut dim = || dims.next().and_then(int).map_or(0.0, |v| to_bp(v, scale));
    let (width, height, depth) = match first {
        '[' | '(' | 'v' | 'h' => (dim(), dim(), dim()),
        'k' => (dim(), 0.0, 0.0),
        _ => (0.0, 0.0, 0.0),
    };
    Some(Record {
        page,
        kind,
        tag,
        line,
        x: to_bp(int(x)?, scale),
        y: to_bp(int(y)?, scale),
        width,
        height,
        depth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Real output from Tectonic for a one-paragraph article.
    const SAMPLE: &str = "SyncTeX Version:1
Input:1:/work/main.tex
Input:2:/work/chap/one.tex
Output:pdf
Magnification:1000
Unit:1
X Offset:0
Y Offset:0
Content:
!254
{1
[1,4:4736287,46220575:26673152,41484288,0
(1,4:4736287,4736287:0,0,0
)
[1,4:8799519,44254495:22609920,36044800,0
(1,3:8799519,8865055:22609920,533465,126483
h1,3:8799519,8865055:983040,0,0
g1,3:11657544,8865055
x1,3:14453437,8865055
k1,3:31409439,8865055:14891394
)
(2,7:8799519,10865055:22609920,533465,126483
g2,7:11657544,10865055
)
]
]
!819
}1
Postamble:
Count:33
";

    #[test]
    fn forward_finds_the_line() {
        let s = Synctex::parse(SAMPLE).expect("parses");
        let hits = s.forward("main.tex", 3, "/work").expect("hit");
        assert_eq!(hits.len(), 1);
        let h = hits[0];
        assert_eq!(h.page, 1);
        // 8799519 sp is 134.27 pt which is 133.8 bp.
        assert!((h.x - 133.8).abs() < 0.5, "{h:?}");
        assert!(h.width > 300.0, "{h:?}");
    }

    #[test]
    fn forward_snaps_to_the_next_line_with_output() {
        let s = Synctex::parse(SAMPLE).expect("parses");
        let hits = s.forward("main.tex", 1, "/work").expect("hit");
        assert!(!hits.is_empty());
        let in_sub = s.forward("chap/one.tex", 7, "/work").expect("hit");
        assert_eq!(in_sub[0].page, 1);
    }

    #[test]
    fn reverse_prefers_the_smallest_containing_box() {
        let s = Synctex::parse(SAMPLE).expect("parses");
        let p = s.forward("chap/one.tex", 7, "/work").expect("hit")[0];
        let src = s
            .reverse(1, p.x + 5.0, p.y + p.height / 2.0, "/work")
            .expect("hit");
        assert_eq!(
            src,
            SourcePosition {
                file: "chap/one.tex".to_owned(),
                line: 7
            }
        );
    }

    #[test]
    fn reverse_credits_text_to_the_file_it_was_read_from() {
        // A paragraph box closed in main.tex:6 holding glue read from one.tex:1.
        let text = "Input:1:/work/main.tex\nInput:2:/work/one.tex\nContent:\n{1\n\
                    (1,6:6553600,6553600:6553600,655360,0\n\
                    g2,1:7000000,6553600\ng2,1:9000000,6553600\n)\n}1\n";
        let s = Synctex::parse(text).expect("parses");
        let at = s.reverse(1, 105.0, 99.0, "/work").expect("hit");
        assert_eq!(at.file, "one.tex");
        assert_eq!(at.line, 1);
    }

    #[test]
    fn unknown_file_and_missing_content_are_errors() {
        let s = Synctex::parse(SAMPLE).expect("parses");
        assert!(matches!(
            s.forward("nope.tex", 1, "/work"),
            Err(LatexError::SynctexNoMatch(_))
        ));
        assert!(Synctex::parse("garbage").is_err());
        assert!(Synctex::from_gz(b"not gzip").is_err());
    }

    proptest! {
        #[test]
        fn arbitrary_text_never_panics(text in any::<String>()) {
            let _ = Synctex::parse(&format!("Content:\n{text}"));
        }

        #[test]
        fn record_shaped_noise_never_panics(
            lines in proptest::collection::vec("[\\[\\(\\{hvkgx$!}\\]\\)]?[0-9,:\\-]{0,30}", 0..40)
        ) {
            let text = format!("Input:1:a.tex\nContent:\n{}", lines.join("\n"));
            if let Ok(s) = Synctex::parse(&text) {
                let _ = s.forward("a.tex", 1, "");
                let _ = s.reverse(1, 10.0, 10.0, "");
            }
        }
    }
}
