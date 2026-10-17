/**
 * CodeMirror side of the kernel sync.
 *
 * Two directions, one rule. User transactions are turned into kernel commands
 * by an update listener; kernel text is applied as a transaction carrying the
 * `fromKernel` annotation, which that same listener ignores. That annotation is
 * the entire echo-loop defence: a change is only ever sent if it did not come
 * from the kernel.
 */

import { Annotation, Transaction, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

import { diffRange } from "$lib/kernel/offsets";
import { toKernelOps, type KernelOp, type KernelSync } from "$lib/kernel/sync";

/** Marks a transaction that applies kernel state and must not be sent back. */
export const fromKernel = Annotation.define<boolean>();

/**
 * Kernel commands for each user transaction, in order. Transactions that came
 * from the kernel, or changed nothing, yield no batch.
 */
export function opsForTransactions(transactions: readonly Transaction[]): KernelOp[][] {
  const batches: KernelOp[][] = [];
  for (const tr of transactions) {
    if (!tr.docChanged || isKernelTransaction(tr)) continue;
    batches.push(toKernelOps(tr.startState.doc, tr.changes));
  }
  return batches;
}

/** Sends every user transaction to the kernel. */
export function kernelSyncExtension(sync: KernelSync): Extension {
  return EditorView.updateListener.of((update) => {
    if (!update.docChanged) return;
    for (const batch of opsForTransactions(update.transactions)) sync.submit(batch);
  });
}

/**
 * Replace the editor's text with the kernel's as a minimal change, so the caret,
 * scroll position and syntax tree survive. Kept out of undo history: undoing a
 * resync would re-create the divergence it just repaired.
 */
export function applyKernelText(view: EditorView, text: string): void {
  const current = view.state.doc.toString();
  if (current === text) return;
  const { from, to, insert } = diffRange(current, text);
  view.dispatch({
    changes: { from, to, insert },
    annotations: [
      fromKernel.of(true),
      Transaction.remote.of(true),
      Transaction.addToHistory.of(false),
    ],
  });
}

/** Whether a transaction came from the kernel. Exposed for tests. */
export function isKernelTransaction(tr: Transaction): boolean {
  return tr.annotation(fromKernel) === true;
}
