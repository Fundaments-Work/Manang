//! The rope bridge: keeps the editor's rope and the CRDT document in step.
//!
//! The rope stays the buffer of record for display, search and compilation.
//! Local edits arrive as rope-level replacements (character indexes, as the
//! editor plugin produces them) or as UTF-16 replacements (as CodeMirror
//! produces them). Remote updates come back as an ordered list of
//! [`AppliedChange`]s carrying both units, so the Rust rope and the web view
//! each take the one they index by.
//!
//! Index conversion is the classic source of bugs here: ropey counts Unicode
//! scalars, CodeMirror counts UTF-16 code units, and the CRDT engines count
//! in one of the two. Every conversion goes through the rope (O(log n)) and
//! is bounds-checked, because ropey panics on out-of-range indexes.

use ropey::Rope;

use crate::engine::{Applied, IndexUnit, RawChange, TextCrdt};
use crate::error::LiveError;

/// What the bridge needs from the buffer it keeps in step.
///
/// Implemented for [`ropey::Rope`]. The production buffer
/// (`manang_buffer::DocumentBuffer`) needs a history-bypassing accessor
/// before it can implement this; see the work-package plan.
pub trait TextTarget {
    /// Length in Unicode scalar values.
    fn len_chars(&self) -> usize;
    /// Length in UTF-16 code units.
    fn len_utf16(&self) -> usize;
    /// UTF-16 offset of a character index (`char_idx <= len_chars`).
    fn char_to_utf16(&self, char_idx: usize) -> usize;
    /// Character index of a UTF-16 offset (`utf16_idx <= len_utf16`).
    fn utf16_to_char(&self, utf16_idx: usize) -> usize;
    /// Replace `start..end` (characters) with `text`.
    fn replace_chars(&mut self, start: usize, end: usize, text: &str);
    /// The whole text, for resynchronisation and tests.
    fn to_text(&self) -> String;
}

impl TextTarget for Rope {
    fn len_chars(&self) -> usize {
        Rope::len_chars(self)
    }
    fn len_utf16(&self) -> usize {
        self.len_utf16_cu()
    }
    fn char_to_utf16(&self, char_idx: usize) -> usize {
        self.char_to_utf16_cu(char_idx)
    }
    fn utf16_to_char(&self, utf16_idx: usize) -> usize {
        self.utf16_cu_to_char(utf16_idx)
    }
    fn replace_chars(&mut self, start: usize, end: usize, text: &str) {
        if end > start {
            self.remove(start..end);
        }
        if !text.is_empty() {
            self.insert(start, text);
        }
    }
    fn to_text(&self) -> String {
        self.to_string()
    }
}

/// One applied replacement in every unit a consumer needs. Indexes refer to
/// the document after all earlier changes in the same list were applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedChange {
    /// Start, in Unicode scalar values (the rope).
    pub char_start: usize,
    /// Replaced length, in Unicode scalar values.
    pub char_delete: usize,
    /// Start, in UTF-16 code units (CodeMirror).
    pub utf16_start: usize,
    /// Replaced length, in UTF-16 code units.
    pub utf16_delete: usize,
    /// Replacement text.
    pub insert: String,
}

/// The result of applying one remote update.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RemoteApplied {
    /// Changes in application order.
    pub changes: Vec<AppliedChange>,
    /// The update needs operations this replica lacks: trigger a resync.
    pub pending: bool,
}

/// A CRDT document bridged to a rope.
pub struct LiveDoc<E: TextCrdt> {
    engine: E,
}

fn check_char_range<T: TextTarget>(t: &T, from: usize, to: usize) -> Result<(), LiveError> {
    let len = t.len_chars();
    if from > to || to > len {
        return Err(LiveError::OutOfRange { from, to, len });
    }
    Ok(())
}

/// UTF-16 offset to character index, refusing offsets inside a surrogate pair.
fn utf16_to_char_checked<T: TextTarget>(t: &T, idx: usize) -> Result<usize, LiveError> {
    let len = t.len_utf16();
    if idx > len {
        return Err(LiveError::OutOfRange {
            from: idx,
            to: idx,
            len,
        });
    }
    let c = t.utf16_to_char(idx);
    if t.char_to_utf16(c) != idx {
        return Err(LiveError::Engine(
            "offset falls inside a surrogate pair".to_owned(),
        ));
    }
    Ok(c)
}

impl<E: TextCrdt> LiveDoc<E> {
    /// Wrap an engine (for example one restored from a snapshot).
    pub fn from_engine(engine: E) -> Self {
        Self { engine }
    }

    /// A host's document, seeded from the file's current text.
    pub fn seeded(peer: u64, text: &str) -> Result<Self, LiveError> {
        Ok(Self::from_engine(E::seeded(peer, text)?))
    }

    /// A joiner's empty document; its content arrives through updates.
    pub fn empty(peer: u64) -> Result<Self, LiveError> {
        Ok(Self::from_engine(E::new(peer)?))
    }

    /// The wrapped engine.
    pub fn engine(&self) -> &E {
        &self.engine
    }

    /// The wrapped engine, mutably (sync protocol use).
    pub fn engine_mut(&mut self) -> &mut E {
        &mut self.engine
    }

    /// Record a local edit that has *already* happened in the rope, given in
    /// UTF-16 units (CodeMirror's). Does not touch the rope.
    pub fn record_local_utf16(
        &mut self,
        start: usize,
        delete: usize,
        insert: &str,
    ) -> Result<(), LiveError> {
        if delete > 0 {
            self.engine.delete(start, delete)?;
        }
        if !insert.is_empty() {
            self.engine.insert(start, insert)?;
        }
        Ok(())
    }

    /// Apply a local edit given as a character range (the editor plugin's
    /// unit) to the rope and the CRDT in one step. This is the keystroke path.
    pub fn local_replace_chars<T: TextTarget>(
        &mut self,
        target: &mut T,
        from: usize,
        to: usize,
        text: &str,
    ) -> Result<(), LiveError> {
        check_char_range(target, from, to)?;
        let u_from = target.char_to_utf16(from);
        let u_to = target.char_to_utf16(to);
        target.replace_chars(from, to, text);
        self.record_local_utf16(u_from, u_to - u_from, text)
    }

    /// Apply a local edit given as a UTF-16 range (CodeMirror's unit).
    pub fn local_replace_utf16<T: TextTarget>(
        &mut self,
        target: &mut T,
        start: usize,
        end: usize,
        text: &str,
    ) -> Result<(), LiveError> {
        let c_from = utf16_to_char_checked(target, start)?;
        let c_to = utf16_to_char_checked(target, end)?;
        if c_from > c_to {
            return Err(LiveError::OutOfRange {
                from: start,
                to: end,
                len: target.len_utf16(),
            });
        }
        target.replace_chars(c_from, c_to, text);
        self.record_local_utf16(start, end - start, text)
    }

    /// Update blobs for local edits not yet sent.
    pub fn take_local_updates(&mut self) -> Result<Vec<Vec<u8>>, LiveError> {
        self.engine.take_local_updates()
    }

    /// Import a remote update, apply its changes to the rope and return them
    /// in both units for the web view.
    pub fn apply_remote<T: TextTarget>(
        &mut self,
        target: &mut T,
        update: &[u8],
    ) -> Result<RemoteApplied, LiveError> {
        let Applied {
            unit,
            changes,
            pending,
        } = self.engine.apply_update(update)?;
        let mut out = Vec::with_capacity(changes.len());
        for change in changes {
            out.push(apply_raw(target, unit, &change)?);
        }
        Ok(RemoteApplied {
            changes: out,
            pending,
        })
    }

    /// Make the rope equal the CRDT text with one minimal replacement. Used
    /// after a snapshot load and as the repair path if the two ever diverge.
    pub fn resync_target<T: TextTarget>(&self, target: &mut T) -> Option<AppliedChange> {
        let have = target.to_text();
        let want = self.engine.text();
        if have == want {
            return None;
        }
        let (start, end_have, insert) = diff_range_chars(&have, &want);
        let utf16_start = target.char_to_utf16(start);
        let utf16_delete = target.char_to_utf16(end_have) - utf16_start;
        target.replace_chars(start, end_have, &insert);
        Some(AppliedChange {
            char_start: start,
            char_delete: end_have - start,
            utf16_start,
            utf16_delete,
            insert,
        })
    }
}

fn apply_raw<T: TextTarget>(
    target: &mut T,
    unit: IndexUnit,
    change: &RawChange,
) -> Result<AppliedChange, LiveError> {
    let (c_start, c_end) = match unit {
        IndexUnit::Chars => (change.start, change.start + change.delete),
        IndexUnit::Utf16 => (
            utf16_to_char_checked(target, change.start)?,
            utf16_to_char_checked(target, change.start + change.delete)?,
        ),
    };
    check_char_range(target, c_start, c_end)?;
    let utf16_start = target.char_to_utf16(c_start);
    let utf16_delete = target.char_to_utf16(c_end) - utf16_start;
    target.replace_chars(c_start, c_end, &change.insert);
    Ok(AppliedChange {
        char_start: c_start,
        char_delete: c_end - c_start,
        utf16_start,
        utf16_delete,
        insert: change.insert.clone(),
    })
}

/// Smallest single replacement turning `have` into `want`, in characters:
/// `(start, end_in_have, replacement)`.
fn diff_range_chars(have: &str, want: &str) -> (usize, usize, String) {
    let h: Vec<char> = have.chars().collect();
    let w: Vec<char> = want.chars().collect();
    let max = h.len().min(w.len());
    let mut start = 0;
    while start < max && h[start] == w[start] {
        start += 1;
    }
    let mut end_h = h.len();
    let mut end_w = w.len();
    while end_h > start && end_w > start && h[end_h - 1] == w[end_w - 1] {
        end_h -= 1;
        end_w -= 1;
    }
    (start, end_h, w[start..end_w].iter().collect())
}

/// Map a UTF-16 offset through a list of sequentially applied changes
/// (cursor mapping; `assoc_after` keeps the offset after an insertion made
/// exactly at it, as a caret typed into would).
#[must_use]
pub fn map_utf16_offset(mut pos: usize, changes: &[AppliedChange], assoc_after: bool) -> usize {
    for c in changes {
        let ins = c.insert.encode_utf16().count();
        let end = c.utf16_start + c.utf16_delete;
        if pos < c.utf16_start || (pos == c.utf16_start && !assoc_after) {
            continue;
        }
        if pos >= end {
            pos = pos - c.utf16_delete + ins;
        } else {
            // Inside a deleted range: collapse to the start of the insertion.
            pos = c.utf16_start;
        }
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LoroEngine, YrsEngine};

    fn two_way<E: TextCrdt>() {
        let mut ra = Rope::from_str("héllo 😀 wörld");
        let mut a = LiveDoc::<E>::seeded(10, &ra.to_string()).expect("a");
        let mut rb = Rope::from_str("héllo 😀 wörld");
        let mut b = LiveDoc::<E>::seeded(11, &rb.to_string()).expect("b");
        // identical seed: no duplication after a full exchange
        let ua = a.engine().export_since(&b.engine().version()).expect("ua");
        let r = b.apply_remote(&mut rb, &ua).expect("apply");
        assert!(r.changes.is_empty(), "seed must not duplicate: {r:?}");
        assert_eq!(a.engine().text(), b.engine().text());

        // a types an emoji in the middle (char index 6 = before the emoji)
        a.local_replace_chars(&mut ra, 6, 6, "🎉").expect("edit");
        let ups = a.take_local_updates().expect("ups");
        for u in &ups {
            let applied = b.apply_remote(&mut rb, u).expect("apply");
            assert_eq!(applied.changes.len(), 1);
            let c = &applied.changes[0];
            assert_eq!((c.char_start, c.utf16_start), (6, 6));
            assert_eq!(c.insert, "🎉");
        }
        assert_eq!(ra.to_string(), rb.to_string());
        // b deletes the first emoji and the space after it, using UTF-16 units
        let start = rb.char_to_utf16_cu(7);
        b.local_replace_utf16(&mut rb, start, start + 3, "")
            .expect("del");
        for u in b.take_local_updates().expect("ups") {
            let applied = a.apply_remote(&mut ra, &u).expect("apply");
            let c = &applied.changes[0];
            assert_eq!((c.char_start, c.char_delete), (7, 2));
            assert_eq!((c.utf16_start, c.utf16_delete), (8, 3));
        }
        assert_eq!(ra.to_string(), rb.to_string());
        assert_eq!(a.engine().text(), ra.to_string());
    }

    #[test]
    fn two_way_loro() {
        two_way::<LoroEngine>();
    }

    #[test]
    fn two_way_yrs() {
        two_way::<YrsEngine>();
    }

    #[test]
    fn surrogate_split_rejected() {
        let mut r = Rope::from_str("a😀");
        let mut d = LiveDoc::<LoroEngine>::seeded(5, "a😀").expect("d");
        assert!(d.local_replace_utf16(&mut r, 2, 2, "x").is_err());
        assert!(d.local_replace_chars(&mut r, 0, 9, "x").is_err());
    }

    #[test]
    fn resync_repairs_divergence() {
        let d = LiveDoc::<YrsEngine>::seeded(5, "hello wörld").expect("d");
        let mut r = Rope::from_str("hello 😀rld");
        let c = d.resync_target(&mut r).expect("change");
        assert_eq!(r.to_string(), "hello wörld");
        assert_eq!(c.char_start, 6);
    }

    #[test]
    fn cursor_mapping() {
        let ch = |s: usize, d: usize, ins: &str| AppliedChange {
            char_start: s,
            char_delete: d,
            utf16_start: s,
            utf16_delete: d,
            insert: ins.to_owned(),
        };
        assert_eq!(map_utf16_offset(5, &[ch(0, 0, "ab")], true), 7);
        assert_eq!(map_utf16_offset(0, &[ch(0, 0, "ab")], true), 2);
        assert_eq!(map_utf16_offset(0, &[ch(0, 0, "ab")], false), 0);
        assert_eq!(map_utf16_offset(5, &[ch(2, 6, "")], true), 2);
        assert_eq!(map_utf16_offset(9, &[ch(2, 3, "xy")], true), 8);
    }
}
