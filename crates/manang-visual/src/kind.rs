//! The span kind vocabulary (ADR 0015 section 3). Ids 0-255 are first-party
//! and frozen; adapters may define their own from [`CUSTOM_BASE`] up.

pub const OPAQUE: u16 = 0;
pub const HEADING: u16 = 1;
pub const STRONG: u16 = 2;
pub const EMPH: u16 = 3;
pub const CODE: u16 = 4;
pub const CODE_BLOCK: u16 = 5;
pub const LIST_ITEM: u16 = 6;
pub const LINK: u16 = 7;
pub const IMAGE: u16 = 8;
pub const QUOTE: u16 = 9;
pub const MATH: u16 = 10;
pub const FRONTMATTER: u16 = 11;
pub const TABLE: u16 = 12;
pub const RULE: u16 = 13;
pub const REF: u16 = 14;
pub const LABEL: u16 = 15;
pub const COMMENT: u16 = 16;
pub const MARKER: u16 = 17;
pub const STRIKE: u16 = 18;
pub const CITE: u16 = 19;

/// First id an adapter may define for itself.
pub const CUSTOM_BASE: u16 = 1024;

/// `aux` of a [`LIST_ITEM`].
pub const LIST_BULLET: u8 = 0;
pub const LIST_ORDERED: u8 = 1;
pub const LIST_TASK_OPEN: u8 = 2;
pub const LIST_TASK_DONE: u8 = 3;

/// `aux` of a [`MATH`] span.
pub const MATH_INLINE: u8 = 0;
pub const MATH_BLOCK: u8 = 1;

/// `aux` of an [`IMAGE`] span that is a figure.
pub const IMAGE_FIGURE: u8 = 1;

/// Every first-party kind with its manifest name.
pub const NAMES: &[(u16, &str)] = &[
    (OPAQUE, "opaque"),
    (HEADING, "heading"),
    (STRONG, "strong"),
    (EMPH, "emph"),
    (CODE, "code"),
    (CODE_BLOCK, "code-block"),
    (LIST_ITEM, "list-item"),
    (LINK, "link"),
    (IMAGE, "image"),
    (QUOTE, "quote"),
    (MATH, "math"),
    (FRONTMATTER, "frontmatter"),
    (TABLE, "table"),
    (RULE, "rule"),
    (REF, "ref"),
    (LABEL, "label"),
    (COMMENT, "comment"),
    (MARKER, "marker"),
    (STRIKE, "strike"),
    (CITE, "cite"),
];

/// The manifest name of a first-party kind id.
pub fn name(kind: u16) -> Option<&'static str> {
    NAMES.iter().find(|(id, _)| *id == kind).map(|(_, n)| *n)
}

/// The id of a first-party kind name.
pub fn id(name: &str) -> Option<u16> {
    NAMES.iter().find(|(_, n)| *n == name).map(|(id, _)| *id)
}

/// Kinds whose content the web view never decorates inside.
pub fn is_leaf(kind: u16) -> bool {
    matches!(
        kind,
        OPAQUE | CODE | CODE_BLOCK | MATH | FRONTMATTER | COMMENT
    )
}
