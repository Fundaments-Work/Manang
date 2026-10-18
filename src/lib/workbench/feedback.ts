/** The text a feedback message turns into. Pure, so it is tested. */

export type FeedbackKind = "feedback" | "problem";

const HEADINGS: Record<FeedbackKind, string> = {
  feedback: "Manang feedback",
  problem: "Manang problem report",
};

export const MESSAGE_PLACEHOLDER: Record<FeedbackKind, string> = {
  feedback: "What would make Manang better for you?",
  problem: "What happened, and what did you expect instead?",
};

/** The message, then the diagnostics report when it is included. */
export function feedbackText(
  kind: FeedbackKind,
  message: string,
  report: string,
): string {
  const parts = [
    HEADINGS[kind],
    message.trim() === "" ? "(no message)" : message.trim(),
  ];
  if (report.trim() !== "") parts.push("--- diagnostics ---", report.trimEnd());
  return `${parts.join("\n\n")}\n`;
}
