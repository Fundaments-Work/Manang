/**
 * What the Agent is, in one word, for the status chip and the view: `ready`, `not-set-up`, `off`
 * (or `checking` before the first answer). Consent counts as part of set-up: until the person
 * has agreed to send data to the provider, the agent is not ready.
 */

import type { AgentStatus } from "./types";

export type Readiness = "checking" | "ready" | "not-set-up" | "off";

export function readiness(status: AgentStatus | null): Readiness {
  if (status === null) return "checking";
  if (status.ready) return "ready";
  if (!status.enabled) return "off";
  return "not-set-up";
}

export const READINESS_LABEL: Readonly<Record<Readiness, string>> = {
  checking: "AI: checking",
  ready: "AI: ready",
  "not-set-up": "AI: not set up",
  off: "AI: off",
};

const TOOL_VERBS: Readonly<Record<string, string>> = {
  "agent.doc-read": "Read",
  "agent.doc-diagnostics": "Checked the problems",
  "agent.doc-outline": "Read the outline",
  "agent.doc-propose-edit": "Proposed an edit",
  "agent.project-search": "Searched the project",
};

/** A tool call as one short line ("Read main.typ"), never the internal command id. */
export function toolLine(name: string, summary: string, state: string): string {
  const verb = TOOL_VERBS[name] ?? name.replace(/^agent\./, "").replace(/[-_.]/g, " ");
  const head = state === "error" ? `Could not run: ${verb.toLowerCase()}` : verb;
  return summary === "" ? head : `${head} ${summary}`;
}
