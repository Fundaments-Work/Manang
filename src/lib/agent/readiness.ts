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
