/**
 * The recovery offer: after a crash or a force-quit, one quiet dialog asks
 * whether to bring back the unsaved edits of the open project.
 *
 * Kept apart from `actions.ts` (which calls it once a project and its tabs are
 * open) and given `open` by the caller, so the two do not import each other.
 */

import { reportError } from "$lib/kernel";
import { recoveryMessage, recoveryTitle } from "$lib/workbench/recovery";
import { basename } from "$lib/workbench/paths";

import { dialog } from "./dialog.svelte";
import { notice } from "./notice.svelte";
import { recovery } from "./recovery.svelte";

/**
 * Offer the open project's recovery snapshots. Recover all opens each as an
 * unsaved edit; Review shows each against the disk; Discard removes them;
 * closing the dialog (Esc, Cancel) leaves them for next time.
 */
export async function offerRecovery(
  open: (path: string) => Promise<boolean>,
): Promise<void> {
  try {
    const entries = await recovery.pending();
    if (entries.length === 0) return;
    const answer = await dialog.ask({
      title: recoveryTitle(entries, Date.now()),
      message: recoveryMessage(entries),
      actions: [
        { id: "recover", label: "Recover all", primary: true },
        { id: "review", label: "Review" },
        { id: "discard", label: "Discard" },
      ],
    });
    if (answer === "recover") {
      let restored = 0;
      for (const entry of entries) {
        if (await recovery.restore(entry, open)) restored += 1;
      }
      if (restored > 0) {
        notice.show(
          `Recovered ${restored} ${restored === 1 ? "file" : "files"}. Review the changes, then save.`,
        );
      }
    } else if (answer === "review") {
      await recovery.openReview(entries);
    } else if (answer === "discard") {
      await recovery.discardAll();
    }
  } catch (error) {
    reportError("recovery.offer", error);
  }
}

/** The review dialog's "Recover this": open the shown snapshot as an unsaved edit. */
export async function recoverShown(
  open: (path: string) => Promise<boolean>,
): Promise<void> {
  const review = recovery.review;
  const item = review?.items[review.index];
  if (item === undefined) return;
  const ok = await recovery.restore(item.entry, open);
  if (ok) notice.show(`Recovered ${basename(item.entry.path)}. Save it to keep it.`);
  recovery.nextReview();
}

/** The review dialog's "Discard": remove the shown snapshot. */
export function discardShown(): void {
  const review = recovery.review;
  const item = review?.items[review.index];
  if (item === undefined) return;
  recovery.forget(item.entry.path);
  recovery.nextReview();
}
