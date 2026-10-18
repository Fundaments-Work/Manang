/**
 * Binary artefacts from the kernel (rendered SVG pages, PDFs).
 *
 * Bytes arrive as a raw `Uint8Array` and become a Blob URL; they are never
 * base64-encoded. A Blob URL pins its bytes in memory until revoked, so the
 * holder revokes the previous one whenever it is replaced or released.
 */

export type ArtifactKind = "svg" | "pdf";

export interface Artifact {
  readonly kind: ArtifactKind;
  readonly mime: string;
}

const MIME: Record<ArtifactKind, string> = {
  svg: "image/svg+xml",
  pdf: "application/pdf",
};

const decoder = new TextDecoder();

/** Identify an artefact from its leading bytes; `null` if it is neither. */
export function sniffArtifact(bytes: Uint8Array): Artifact | null {
  // "%PDF-" may be preceded by a few junk bytes; the spec allows 1024.
  const head = decoder.decode(bytes.subarray(0, 1024));
  if (head.includes("%PDF-")) return { kind: "pdf", mime: MIME.pdf };
  // `trimStart` already strips a leading byte-order mark (U+FEFF is whitespace).
  // Leading comments too: the SVGs Manang writes next to diagrams start with
  // `<!-- manang-diagram ... -->`.
  const trimmed = head.trimStart().replace(/^(?:<!--[\s\S]*?-->\s*)+/, "");
  if (
    trimmed.startsWith("<svg") ||
    (trimmed.startsWith("<?xml") && head.includes("<svg"))
  ) {
    return { kind: "svg", mime: MIME.svg };
  }
  return null;
}

export interface UrlApi {
  create(blob: Blob): string;
  revoke(url: string): void;
}

export const browserUrls: UrlApi = {
  create: (blob) => URL.createObjectURL(blob),
  revoke: (url) => URL.revokeObjectURL(url),
};

/** Owns at most one live Blob URL. */
export class ArtifactUrl {
  readonly #api: UrlApi;
  #url: string | null = null;

  constructor(api: UrlApi = browserUrls) {
    this.#api = api;
  }

  get url(): string | null {
    return this.#url;
  }

  /** Replace the held artefact; the previous URL is revoked. */
  set(bytes: Uint8Array, mime: string): string {
    this.release();
    // Copy into a plain ArrayBuffer view so the Blob never aliases IPC memory.
    const url = this.#api.create(new Blob([new Uint8Array(bytes)], { type: mime }));
    this.#url = url;
    return url;
  }

  release(): void {
    if (this.#url !== null) this.#api.revoke(this.#url);
    this.#url = null;
  }
}
