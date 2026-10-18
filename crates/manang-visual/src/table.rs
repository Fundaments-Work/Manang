//! The span table: normalisation and the frozen binary encoding
//! (ADR 0015 section 3, wire version 1).

use std::collections::HashMap;

use thiserror::Error;

use crate::adapter::RawSpan;

/// Magic bytes at the start of every table.
pub const MAGIC: [u8; 4] = *b"MVS1";
/// Wire version.
pub const VERSION: u16 = 1;
/// Header size in bytes.
pub const HEADER_BYTES: usize = 40;
/// One span record in bytes.
pub const RECORD_BYTES: usize = 16;
/// `flags` bit 0: the span cap was hit.
pub const FLAG_TRUNCATED: u16 = 1;
/// Most spans one table holds.
pub const MAX_SPANS: usize = 65_536;
/// `attr` value meaning "no string".
pub const NO_ATTR: u32 = u32::MAX;

/// One record of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub kind: u16,
    pub layer: u8,
    pub aux: u8,
    pub attr: u32,
}

/// A decoded or freshly built table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanTable {
    pub revision: u64,
    pub from: u32,
    pub to: u32,
    pub doc_len: u32,
    pub truncated: bool,
    pub spans: Vec<Span>,
    pub attrs: Vec<String>,
}

/// A byte string is not a valid span table.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TableError {
    #[error("span table is {0} bytes, shorter than its header or declared size")]
    Short(usize),
    #[error("span table has the wrong magic")]
    Magic,
    #[error("span table version {0} is not supported")]
    Version(u16),
    #[error("span table string table is malformed")]
    Strings,
}

impl SpanTable {
    /// Build a table from raw spans whose offsets are already UTF-16 units.
    ///
    /// Enforces every invariant the wire format promises: spans are clamped to
    /// `0..doc_len`, empty ones dropped, those outside the viewport dropped,
    /// the rest sorted ancestors-first, made laminar (a span that crosses its
    /// parent's end is cut at it) and given their nesting depth as `layer`.
    pub fn build(
        mut raw: Vec<RawSpan>,
        doc_len: usize,
        from: usize,
        to: usize,
        revision: u64,
    ) -> Self {
        let doc_len = doc_len.min(u32::MAX as usize);
        let from = from.min(doc_len);
        let to = to.clamp(from, doc_len);
        let (view_from, view_to) = if from == to {
            (from, from.saturating_add(1))
        } else {
            (from, to)
        };

        raw.retain_mut(|s| {
            s.range.end = s.range.end.min(doc_len);
            s.range.start < s.range.end && s.range.start < view_to && s.range.end > view_from
        });
        // Stable: equal ranges keep the adapter's order, parent first.
        raw.sort_by(|a, b| {
            a.range
                .start
                .cmp(&b.range.start)
                .then(b.range.end.cmp(&a.range.end))
        });

        let mut truncated = false;
        let mut spans = Vec::with_capacity(raw.len().min(MAX_SPANS));
        let mut attrs: Vec<String> = Vec::new();
        let mut interned: HashMap<String, u32> = HashMap::new();
        let mut stack: Vec<usize> = Vec::new(); // ends of the open ancestors
        for s in raw {
            if spans.len() == MAX_SPANS {
                truncated = true;
                break;
            }
            let start = s.range.start;
            while stack.last().is_some_and(|&end| end <= start) {
                stack.pop();
            }
            let mut end = s.range.end;
            if let Some(&parent_end) = stack.last() {
                end = end.min(parent_end);
            }
            if end <= start {
                continue;
            }
            let attr = match s.attr {
                None => NO_ATTR,
                Some(text) => match interned.get(&text) {
                    Some(&ix) => ix,
                    None => {
                        let ix = u32::try_from(attrs.len()).unwrap_or(NO_ATTR);
                        interned.insert(text.clone(), ix);
                        attrs.push(text);
                        ix
                    }
                },
            };
            spans.push(Span {
                // Both fit: they are at most `doc_len`, itself at most u32::MAX.
                start: start as u32,
                end: end as u32,
                kind: s.kind,
                layer: u8::try_from(stack.len()).unwrap_or(u8::MAX),
                aux: s.aux,
                attr,
            });
            stack.push(end);
        }

        Self {
            revision,
            from: from as u32,
            to: to as u32,
            doc_len: doc_len as u32,
            truncated,
            spans,
            attrs,
        }
    }

    /// The exact binary encoding of the wire format.
    pub fn encode(&self) -> Vec<u8> {
        let string_bytes: usize = self.attrs.iter().map(String::len).sum();
        let total =
            HEADER_BYTES + self.spans.len() * RECORD_BYTES + self.attrs.len() * 4 + string_bytes;
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        let flags = if self.truncated { FLAG_TRUNCATED } else { 0 };
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&self.revision.to_le_bytes());
        out.extend_from_slice(&self.from.to_le_bytes());
        out.extend_from_slice(&self.to.to_le_bytes());
        out.extend_from_slice(&self.doc_len.to_le_bytes());
        out.extend_from_slice(&(self.spans.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.attrs.len() as u32).to_le_bytes());
        out.extend_from_slice(&(string_bytes as u32).to_le_bytes());
        for s in &self.spans {
            out.extend_from_slice(&s.start.to_le_bytes());
            out.extend_from_slice(&s.end.to_le_bytes());
            let word = u32::from(s.kind) | (u32::from(s.layer) << 16) | (u32::from(s.aux) << 24);
            out.extend_from_slice(&word.to_le_bytes());
            out.extend_from_slice(&s.attr.to_le_bytes());
        }
        let mut end = 0u32;
        for a in &self.attrs {
            end += a.len() as u32;
            out.extend_from_slice(&end.to_le_bytes());
        }
        for a in &self.attrs {
            out.extend_from_slice(a.as_bytes());
        }
        debug_assert_eq!(out.len(), total);
        out
    }

    /// Decode what [`encode`](Self::encode) wrote.
    pub fn decode(bytes: &[u8]) -> Result<Self, TableError> {
        let short = || TableError::Short(bytes.len());
        let u16_at = |o: usize| -> Result<u16, TableError> {
            let b = bytes.get(o..o + 2).ok_or_else(short)?;
            Ok(u16::from_le_bytes([b[0], b[1]]))
        };
        let u32_at = |o: usize| -> Result<u32, TableError> {
            let b = bytes.get(o..o + 4).ok_or_else(short)?;
            Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        if bytes.len() < HEADER_BYTES {
            return Err(short());
        }
        if bytes[..4] != MAGIC {
            return Err(TableError::Magic);
        }
        let version = u16_at(4)?;
        if version != VERSION {
            return Err(TableError::Version(version));
        }
        let flags = u16_at(6)?;
        let revision = u64::from(u32_at(8)?) | (u64::from(u32_at(12)?) << 32);
        let span_count = u32_at(28)? as usize;
        let string_count = u32_at(32)? as usize;
        let string_bytes = u32_at(36)? as usize;
        let ends_at = HEADER_BYTES + span_count * RECORD_BYTES;
        let blob_at = ends_at + string_count * 4;
        if bytes.len() != blob_at + string_bytes {
            return Err(short());
        }
        let mut spans = Vec::with_capacity(span_count);
        for i in 0..span_count {
            let o = HEADER_BYTES + i * RECORD_BYTES;
            let word = u32_at(o + 8)?;
            spans.push(Span {
                start: u32_at(o)?,
                end: u32_at(o + 4)?,
                kind: (word & 0xFFFF) as u16,
                layer: ((word >> 16) & 0xFF) as u8,
                aux: (word >> 24) as u8,
                attr: u32_at(o + 12)?,
            });
        }
        let blob = &bytes[blob_at..];
        let mut attrs = Vec::with_capacity(string_count);
        let mut prev = 0usize;
        for i in 0..string_count {
            let end = u32_at(ends_at + i * 4)? as usize;
            let part = blob.get(prev..end).ok_or(TableError::Strings)?;
            attrs.push(
                std::str::from_utf8(part)
                    .map_err(|_| TableError::Strings)?
                    .to_owned(),
            );
            prev = end;
        }
        Ok(Self {
            revision,
            from: u32_at(16)?,
            to: u32_at(20)?,
            doc_len: u32_at(24)?,
            truncated: flags & FLAG_TRUNCATED != 0,
            spans,
            attrs,
        })
    }

    /// The string of a span's `attr`, if it has one.
    pub fn attr_of(&self, span: &Span) -> Option<&str> {
        self.attrs.get(span.attr as usize).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind;
    use proptest::prelude::*;

    fn raw(start: usize, end: usize, k: u16) -> RawSpan {
        RawSpan::new(start..end, k)
    }

    #[test]
    fn nests_sorts_and_layers() {
        let table = SpanTable::build(
            vec![
                raw(10, 12, kind::MARKER),
                raw(0, 20, kind::HEADING),
                raw(5, 9, kind::STRONG),
                raw(0, 2, kind::MARKER),
            ],
            20,
            0,
            20,
            3,
        );
        let got: Vec<_> = table
            .spans
            .iter()
            .map(|s| (s.start, s.end, s.layer))
            .collect();
        assert_eq!(got, vec![(0, 20, 0), (0, 2, 1), (5, 9, 1), (10, 12, 1)]);
    }

    #[test]
    fn crossing_spans_are_cut_at_the_parent() {
        let table = SpanTable::build(
            vec![raw(0, 10, kind::STRONG), raw(5, 15, kind::EMPH)],
            20,
            0,
            20,
            0,
        );
        assert_eq!((table.spans[1].start, table.spans[1].end), (5, 10));
    }

    #[test]
    fn clamps_drops_and_filters_by_viewport() {
        let table = SpanTable::build(
            vec![
                raw(0, 99, kind::HEADING),
                raw(5, 5, kind::STRONG),
                raw(30, 40, kind::EMPH),
            ],
            10,
            2,
            4,
            0,
        );
        assert_eq!(table.spans.len(), 1);
        assert_eq!((table.spans[0].start, table.spans[0].end), (0, 10));
    }

    #[test]
    fn caret_viewport_counts_as_one_unit() {
        let table = SpanTable::build(vec![raw(3, 6, kind::STRONG)], 10, 3, 3, 0);
        assert_eq!(table.spans.len(), 1);
    }

    #[test]
    fn interns_attrs_and_round_trips() {
        let table = SpanTable::build(
            vec![
                RawSpan::new(0..4, kind::LINK).attr("https://x.org"),
                RawSpan::new(5..9, kind::LINK).attr("https://x.org"),
                RawSpan::new(10..12, kind::MATH).attr("é"),
                raw(13, 15, kind::STRONG),
            ],
            20,
            0,
            20,
            u64::MAX - 1,
        );
        assert_eq!(table.attrs, vec!["https://x.org", "é"]);
        let bytes = table.encode();
        assert_eq!(&bytes[..4], b"MVS1");
        assert_eq!(SpanTable::decode(&bytes), Ok(table));
    }

    #[test]
    fn exact_layout_of_a_tiny_table() {
        let table = SpanTable::build(
            vec![RawSpan::new(1..3, kind::HEADING).aux(2).attr("ab")],
            5,
            0,
            5,
            7,
        );
        let b = table.encode();
        assert_eq!(b.len(), 40 + 16 + 4 + 2);
        assert_eq!(&b[4..6], &1u16.to_le_bytes());
        assert_eq!(&b[8..16], &7u64.to_le_bytes());
        assert_eq!(&b[24..28], &5u32.to_le_bytes());
        assert_eq!(&b[28..32], &1u32.to_le_bytes());
        assert_eq!(&b[40..44], &1u32.to_le_bytes());
        assert_eq!(&b[44..48], &3u32.to_le_bytes());
        assert_eq!(&b[48..52], &(1u32 | (2 << 24)).to_le_bytes());
        assert_eq!(&b[52..56], &0u32.to_le_bytes());
        assert_eq!(&b[56..60], &2u32.to_le_bytes());
        assert_eq!(&b[60..], b"ab");
    }

    #[test]
    fn truncation_is_flagged() {
        let raw: Vec<_> = (0..MAX_SPANS + 5)
            .map(|i| RawSpan::new(i * 2..i * 2 + 1, kind::STRONG))
            .collect();
        let table = SpanTable::build(raw, 2 * (MAX_SPANS + 5), 0, usize::MAX, 0);
        assert!(table.truncated);
        assert_eq!(table.spans.len(), MAX_SPANS);
        let back = SpanTable::decode(&table.encode());
        assert_eq!(back.map(|t| t.truncated), Ok(true));
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(matches!(
            SpanTable::decode(b"nope"),
            Err(TableError::Short(4))
        ));
        let mut bytes = SpanTable::build(vec![], 0, 0, 0, 0).encode();
        bytes[0] = b'X';
        assert_eq!(SpanTable::decode(&bytes), Err(TableError::Magic));
    }

    proptest! {
        /// Whatever an adapter returns, the table obeys the wire invariants.
        #[test]
        fn build_always_yields_a_laminar_ordered_in_bounds_table(
            doc_len in 0usize..200,
            from in 0usize..250,
            to in 0usize..250,
            spans in proptest::collection::vec((0usize..250, 0usize..250, 0u16..20), 0..60),
        ) {
            let raw = spans.into_iter().map(|(a, b, k)| RawSpan::new(a..b, k)).collect();
            let t = SpanTable::build(raw, doc_len, from, to, 1);
            for (i, s) in t.spans.iter().enumerate() {
                prop_assert!(s.start < s.end && s.end as usize <= doc_len);
                if i > 0 {
                    let p = t.spans[i - 1];
                    prop_assert!((p.start, std::cmp::Reverse(p.end)) <= (s.start, std::cmp::Reverse(s.end)));
                }
                // Laminar: against every earlier span, nested or disjoint.
                for q in &t.spans[..i] {
                    let disjoint = q.end <= s.start;
                    let nested = s.end <= q.end;
                    prop_assert!(disjoint || nested);
                    if nested && !disjoint {
                        prop_assert!(s.layer > q.layer);
                    }
                    if s.layer == q.layer {
                        prop_assert!(disjoint);
                    }
                }
            }
            prop_assert_eq!(SpanTable::decode(&t.encode()), Ok(t));
        }
    }
}
