/**
 * Wire contract of the Visual editor (ADR 0015, frozen at version 1).
 *
 * Types and constants only, no logic. The Rust side is `crates/manang-visual`
 * (`table.rs` encodes the span table) and `crates/manang-visual-plugin` (the
 * `visual.*` kernel commands). All offsets are UTF-16 code units, the unit of
 * CodeMirror. JSON commands go through `kernel.invoke`, `visual.blocks`
 * through `kernel.invokeBinary`.
 */

/** Span kind ids. 0-255 are the first-party vocabulary, 1024+ adapter-defined. */
export const SPAN_KIND = {
  opaque: 0,
  heading: 1,
  strong: 2,
  emph: 3,
  code: 4,
  codeBlock: 5,
  listItem: 6,
  link: 7,
  image: 8,
  quote: 9,
  math: 10,
  frontmatter: 11,
  table: 12,
  rule: 13,
  ref: 14,
  label: 15,
  comment: 16,
  marker: 17,
  strike: 18,
  cite: 19,
} as const;

/** A first-party kind id, or an adapter-defined one (1024 and above). */
export type SpanKindId = (typeof SPAN_KIND)[keyof typeof SPAN_KIND] | (number & {});

/** `aux` of a `listItem` span. */
export const LIST_AUX = { bullet: 0, ordered: 1, taskOpen: 2, taskDone: 3 } as const;

/** `aux` of a `math` span. */
export const MATH_AUX = { inline: 0, block: 1 } as const;

/** `aux` of an `image` span: 1 when it is a figure. */
export const IMAGE_AUX = { image: 0, figure: 1 } as const;

/** `attr` value meaning "no string". */
export const NO_ATTR = 0xffff_ffff;

/** Table constants: header bytes, record bytes, magic, version, truncated flag. */
export const SPAN_TABLE = {
  headerBytes: 40,
  recordBytes: 16,
  magic: "MVS1",
  version: 1,
  flagTruncated: 1,
  maxSpans: 65_536,
} as const;

/**
 * Byte layout of the response of `visual.blocks` (little-endian):
 *
 * - header (40 bytes): magic `"MVS1"` | u16 version | u16 flags |
 *   u64 revision | u32 from | u32 to | u32 docLen | u32 spanCount |
 *   u32 stringCount | u32 stringBytes
 * - `spanCount` records of 4 u32 words:
 *   `[start, end, kind | layer << 16 | aux << 24, attr]` (a `Uint32Array`
 *   view at byte offset 40)
 * - `stringCount` u32 cumulative end offsets, then `stringBytes` UTF-8 bytes.
 *
 * Records are sorted by start asc, end desc (ancestors first) and form a
 * laminar family: `layer` is the number of enclosing spans, spans of one layer
 * never overlap. A slice holds every span intersecting `[from, to)`, whole.
 */
export interface SpanTableHeader {
  readonly version: number;
  readonly truncated: boolean;
  readonly revision: number;
  readonly from: number;
  readonly to: number;
  readonly docLen: number;
  readonly spanCount: number;
  readonly stringCount: number;
  readonly stringBytes: number;
}

/** One decoded record. */
export interface Span {
  readonly start: number;
  readonly end: number;
  readonly kind: SpanKindId;
  readonly layer: number;
  readonly aux: number;
  /** Decoded string table entry, or null. */
  readonly attr: string | null;
}

/** CodeMirror `ChangeSpec` semantics: ascending, non-overlapping, positions in the pre-batch document. */
export interface TextChange {
  readonly from: number;
  readonly to: number;
  readonly insert: string;
}

export interface Range {
  readonly from: number;
  readonly to: number;
}

/** `visual.adapters` */
export interface AdapterInfo {
  readonly id: string;
  readonly title: string;
  readonly extensions: readonly string[];
  readonly kinds: readonly string[];
  readonly commands: readonly string[];
}

/** `visual.open` args: the editor document `doc`, or inline `text`; adapter by id or by `path` extension. */
export interface OpenArgs {
  readonly doc?: number;
  readonly text?: string;
  readonly adapter?: string;
  readonly path?: string;
}
export interface OpenResult {
  readonly session: number;
  readonly adapter: string;
  readonly revision: number;
  readonly len: number;
}

/** `visual.reset` args and result. */
export interface ResetArgs {
  readonly session: number;
  readonly doc?: number;
  readonly text?: string;
}
export interface ResetResult {
  readonly session: number;
  readonly revision: number;
  readonly len: number;
}

/** `visual.edit` args: `len` is the document length after the changes. */
export interface EditArgs {
  readonly session: number;
  readonly changes: readonly TextChange[];
  readonly len: number;
}
export interface EditResult {
  readonly revision: number;
  readonly len: number;
  /** Changed region in the new document, or null. Re-request `visual.blocks` if it meets the viewport. */
  readonly dirty: Range | null;
}

/** `visual.blocks` args (result is the binary span table). */
export interface BlocksArgs {
  readonly session: number;
  readonly from: number;
  readonly to: number;
}

/** `visual.outline` */
export interface OutlineEntry {
  readonly level: number;
  readonly title: string;
  readonly from: number;
  readonly to: number;
}

/** `visual.command-state` */
export interface CommandState {
  readonly command: string;
  readonly enabled: boolean;
  readonly active: boolean;
}

/** `visual.run-command` args. `args` is the command's single string argument. */
export interface RunCommandArgs {
  readonly session: number;
  readonly command: string;
  readonly from: number;
  readonly to: number;
  readonly args?: string;
}
/** Dispatch `changes` as one transaction; `selection` is in the document after the changes. */
export interface EditPlan {
  readonly changes: readonly TextChange[];
  readonly selection: Range | null;
}

/** Adapter command ids the toolbar maps from `visual.<id>`. */
export type AdapterCommand =
  | "bold"
  | "italic"
  | "code"
  | "strike"
  | "heading"
  | "list"
  | "quote"
  | "link"
  | "image"
  | "figure"
  | "math"
  | "rule";

/** Error codes in the `[code] message` prefix of a failed command. */
export type VisualErrorCode =
  | "visual.no-session"
  | "visual.no-adapter"
  | "visual.bad-args"
  | "visual.desync"
  | "visual.unsupported"
  | "visual.too-large"
  | "visual.failed";
