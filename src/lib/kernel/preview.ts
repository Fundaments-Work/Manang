/**
 * Preview rendering, independent of any UI framework.
 *
 * The producing command is configurable: `typst.render` and `latex.export-pdf`
 * are built by other plugins and may be absent, which is a normal state ("empty"),
 * not an error. Only a command that exists and then fails is an error.
 */

import type { Kernel } from "./client";
import { sniffArtifact, type Artifact } from "./artifact";

/** Commands tried in order when none is configured. */
export const DEFAULT_PREVIEW_COMMANDS: readonly string[] = [
  "typst.render",
  "latex.export-pdf",
];

export type PreviewResult =
  | { readonly state: "empty" }
  | { readonly state: "error"; readonly message: string }
  | {
      readonly state: "ready";
      readonly bytes: Uint8Array;
      readonly artifact: Artifact;
    };

/** The first candidate the kernel actually offers, if any. */
export async function pickPreviewCommand(
  kernel: Kernel,
  candidates: readonly string[],
): Promise<string | null> {
  try {
    const available = new Set((await kernel.commands()).map((c) => c.id));
    return candidates.find((id) => available.has(id)) ?? null;
  } catch {
    return null;
  }
}

export async function renderPreview(
  kernel: Kernel,
  command: string | null,
  args?: Record<string, unknown>,
): Promise<PreviewResult> {
  if (command === null) return { state: "empty" };
  try {
    const bytes = await kernel.runBinary(command, args);
    const artifact = sniffArtifact(bytes);
    if (artifact === null) {
      return {
        state: "error",
        message: `${command} returned an unrecognised artefact`,
      };
    }
    return { state: "ready", bytes, artifact };
  } catch (error) {
    const message =
      typeof error === "object" && error !== null && "message" in error
        ? String((error as { message: unknown }).message)
        : String(error);
    return { state: "error", message };
  }
}
