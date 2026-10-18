/**
 * pdf.js, loaded once on first use with its worker as a same-origin asset
 * (ADR 0006): no CDN, no eval, nothing fetched at runtime.
 *
 * Shape from Theorem (`src/core/lib/pdfjs-runtime.ts`, MIT, same owner): one
 * shared module promise, the worker configured once.
 */

import type * as PdfJsModule from "pdfjs-dist";

type PdfJs = typeof PdfJsModule;

let loading: Promise<PdfJs> | null = null;

export function pdfjs(): Promise<PdfJs> {
  loading ??= Promise.all([
    import("pdfjs-dist"),
    import("pdfjs-dist/build/pdf.worker.min.mjs?url"),
  ])
    .then(([module, worker]) => {
      module.GlobalWorkerOptions.workerSrc = worker.default;
      return module;
    })
    .catch((error: unknown) => {
      // Let a later call try again (a transient chunk-load failure).
      loading = null;
      throw error;
    });
  return loading;
}

/** Options for every document: offline, no XFA forms. */
export const DOCUMENT_OPTIONS = {
  enableXfa: false,
  useWorkerFetch: false,
} as const;
