/**
 * Decoder of the binary span table (`visual.blocks`, ADR 0015 section 3).
 *
 * The table comes from another process, so nothing in it is trusted: the magic,
 * the version, every length and every offset are checked, and any violation
 * yields `{ ok: false }` instead of an exception. The caller then keeps showing
 * plain source, which is always correct; a visual layer that crashed the view
 * update would not be.
 *
 * Reads go through a `DataView`, so the bytes need no particular alignment
 * (a `Uint8Array` handed over by IPC may start at any offset of its buffer).
 */

import { NO_ATTR, SPAN_TABLE, type Span, type SpanTableHeader } from "./protocol";

/** A decoded, validated table. Spans are sorted: start ascending, end descending. */
export interface SpanTable {
  readonly header: SpanTableHeader;
  readonly spans: readonly Span[];
}

export type DecodeResult =
  | { readonly ok: true; readonly table: SpanTable }
  | { readonly ok: false; readonly reason: string };

const fail = (reason: string): DecodeResult => ({ ok: false, reason });

/** Decode and validate; never throws. */
export function decodeSpanTable(bytes: Uint8Array): DecodeResult {
  try {
    return decode(bytes);
  } catch (error) {
    return fail(error instanceof Error ? error.message : "malformed span table");
  }
}

function decode(bytes: Uint8Array): DecodeResult {
  const { headerBytes, recordBytes } = SPAN_TABLE;
  if (bytes.byteLength < headerBytes) return fail("truncated header");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  for (let i = 0; i < 4; i += 1) {
    if (view.getUint8(i) !== SPAN_TABLE.magic.charCodeAt(i)) return fail("bad magic");
  }
  const version = view.getUint16(4, true);
  if (version !== SPAN_TABLE.version) return fail(`unsupported version ${version}`);
  const flags = view.getUint16(6, true);
  const revision = Number(view.getBigUint64(8, true));
  const from = view.getUint32(16, true);
  const to = view.getUint32(20, true);
  const docLen = view.getUint32(24, true);
  const spanCount = view.getUint32(28, true);
  const stringCount = view.getUint32(32, true);
  const stringBytes = view.getUint32(36, true);
  if (!Number.isSafeInteger(revision)) return fail("revision out of range");
  if (from > to || to > docLen) return fail("viewport outside the document");

  // Sizes are summed in doubles: no u32 overflow can make a short buffer pass.
  const recordsEnd = headerBytes + spanCount * recordBytes;
  const endsEnd = recordsEnd + stringCount * 4;
  const total = endsEnd + stringBytes;
  if (total !== bytes.byteLength) {
    return fail(total > bytes.byteLength ? "truncated table" : "trailing bytes");
  }

  const ends = new Array<number>(stringCount);
  let previous = 0;
  for (let i = 0; i < stringCount; i += 1) {
    const end = view.getUint32(recordsEnd + i * 4, true);
    if (end < previous || end > stringBytes) return fail("bad string offsets");
    ends[i] = end;
    previous = end;
  }
  if (previous !== stringBytes) return fail("bad string length");

  const decoder = new TextDecoder("utf-8");
  const strings = new Array<string | undefined>(stringCount);
  const stringAt = (index: number): string | null => {
    if (index === NO_ATTR) return null;
    if (index >= stringCount) throw new Error("attribute index out of range");
    let text = strings[index];
    if (text === undefined) {
      const start = index === 0 ? 0 : (ends[index - 1] ?? 0);
      text = decoder.decode(
        bytes.subarray(endsEnd + start, endsEnd + (ends[index] ?? start)),
      );
      strings[index] = text;
    }
    return text;
  };

  const spans = new Array<Span>(spanCount);
  // Open ancestors' ends, to prove the family is laminar (nested or disjoint).
  const open: number[] = [];
  let prevStart = 0;
  let prevEnd = 0;
  for (let i = 0; i < spanCount; i += 1) {
    const at = headerBytes + i * recordBytes;
    const start = view.getUint32(at, true);
    const end = view.getUint32(at + 4, true);
    const word = view.getUint32(at + 8, true);
    const attr = view.getUint32(at + 12, true);
    if (start >= end || end > docLen) return fail("span outside the document");
    if (i > 0 && (start < prevStart || (start === prevStart && end > prevEnd))) {
      return fail("spans out of order");
    }
    while (open.length > 0 && (open[open.length - 1] ?? 0) <= start) open.pop();
    const parentEnd = open[open.length - 1];
    if (parentEnd !== undefined && end > parentEnd) return fail("spans cross");
    open.push(end);
    prevStart = start;
    prevEnd = end;
    spans[i] = {
      start,
      end,
      kind: word & 0xffff,
      layer: (word >>> 16) & 0xff,
      aux: (word >>> 24) & 0xff,
      attr: stringAt(attr),
    };
  }

  return {
    ok: true,
    table: {
      header: {
        version,
        truncated: (flags & SPAN_TABLE.flagTruncated) !== 0,
        revision,
        from,
        to,
        docLen,
        spanCount,
        stringCount,
        stringBytes,
      },
      spans,
    },
  };
}

/** What `encodeSpanTable` takes: a span with its layer and optional string. */
export interface EncodableSpan {
  readonly start: number;
  readonly end: number;
  readonly kind: number;
  readonly layer: number;
  readonly aux?: number;
  readonly attr?: string | null;
}

/**
 * Encode a table. The real encoder is Rust; this exists for tests and for the
 * browser mock kernel, which must produce exactly the bytes the contract
 * describes so the decoder is exercised the same way in both.
 */
export function encodeSpanTable(input: {
  readonly revision: number;
  readonly from: number;
  readonly to: number;
  readonly docLen: number;
  readonly truncated?: boolean;
  readonly spans: readonly EncodableSpan[];
}): Uint8Array {
  const encoder = new TextEncoder();
  const strings: Uint8Array[] = [];
  const index = new Map<string, number>();
  const attrIndex = (text: string | null | undefined): number => {
    if (text === null || text === undefined) return NO_ATTR;
    let found = index.get(text);
    if (found === undefined) {
      found = strings.length;
      strings.push(encoder.encode(text));
      index.set(text, found);
    }
    return found;
  };
  const records = input.spans.map((s) => ({ ...s, attrIx: attrIndex(s.attr) }));
  const stringBytes = strings.reduce((n, s) => n + s.length, 0);
  const total =
    SPAN_TABLE.headerBytes +
    records.length * SPAN_TABLE.recordBytes +
    strings.length * 4 +
    stringBytes;
  const out = new Uint8Array(total);
  const view = new DataView(out.buffer);
  for (let i = 0; i < 4; i += 1) out[i] = SPAN_TABLE.magic.charCodeAt(i);
  view.setUint16(4, SPAN_TABLE.version, true);
  view.setUint16(6, input.truncated === true ? SPAN_TABLE.flagTruncated : 0, true);
  view.setBigUint64(8, BigInt(input.revision), true);
  view.setUint32(16, input.from, true);
  view.setUint32(20, input.to, true);
  view.setUint32(24, input.docLen, true);
  view.setUint32(28, records.length, true);
  view.setUint32(32, strings.length, true);
  view.setUint32(36, stringBytes, true);
  let at = SPAN_TABLE.headerBytes;
  for (const r of records) {
    view.setUint32(at, r.start, true);
    view.setUint32(at + 4, r.end, true);
    view.setUint32(
      at + 8,
      ((r.kind & 0xffff) | ((r.layer & 0xff) << 16) | (((r.aux ?? 0) & 0xff) << 24)) >>>
        0,
      true,
    );
    view.setUint32(at + 12, r.attrIx, true);
    at += SPAN_TABLE.recordBytes;
  }
  let sum = 0;
  for (const s of strings) {
    sum += s.length;
    view.setUint32(at, sum, true);
    at += 4;
  }
  for (const s of strings) {
    out.set(s, at);
    at += s.length;
  }
  return out;
}
