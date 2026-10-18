/**
 * The Agent view's transcript as a pure reducer over `agent.*` events.
 *
 * Events carry a `session_id`; the reducer ignores any other session, so a late event of a
 * stopped conversation never lands in the new one. State is immutable.
 */

import type { TextEdit } from "./types";

export type ItemState = "running" | "done" | "error";

export type TranscriptItem =
  | { readonly kind: "user"; readonly text: string; readonly chips: readonly string[] }
  | {
      readonly kind: "assistant";
      readonly text: string;
      readonly thinking: string;
      readonly open: boolean;
    }
  | {
      readonly kind: "tool";
      readonly id: string;
      readonly name: string;
      readonly risk: string;
      readonly state: ItemState;
      readonly summary: string;
    }
  | {
      readonly kind: "approval";
      readonly callId: string;
      readonly tool: string;
      readonly risk: string;
      readonly scope: string;
      readonly afterUntrusted: boolean;
      readonly answer: "pending" | "allowed" | "denied";
    }
  | {
      readonly kind: "proposal";
      readonly proposalId: string;
      readonly proposalKind: string;
      readonly edit: TextEdit | null;
      readonly state: "pending" | "accepted" | "rejected" | "stale";
    }
  | {
      readonly kind: "error";
      readonly code: string;
      readonly message: string;
      readonly requestId: string | null;
      readonly retryable: boolean;
    }
  | { readonly kind: "notice"; readonly text: string };

export interface Usage {
  readonly input: number;
  readonly output: number;
  readonly cacheRead: number;
  readonly cacheWrite: number;
  /** Micro-USD; null when the model has no price. Always an estimate. */
  readonly costMicroUsd: number | null;
}

export interface AgentViewState {
  readonly sessionId: string | null;
  /** `agent.start` was sent and its first event has not arrived: adopt that session. */
  readonly starting: boolean;
  readonly running: boolean;
  readonly items: readonly TranscriptItem[];
  readonly usage: Usage;
  readonly provider: string;
  readonly model: string;
  readonly stopReason: string | null;
}

export const EMPTY_USAGE: Usage = {
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  costMicroUsd: 0,
};

export function initialState(): AgentViewState {
  return {
    sessionId: null,
    starting: false,
    running: false,
    items: [],
    usage: EMPTY_USAGE,
    provider: "",
    model: "",
    stopReason: null,
  };
}

/** The user sent a message: show it and wait for the session. */
export function begin(
  state: AgentViewState,
  text: string,
  chips: readonly string[],
): AgentViewState {
  return {
    ...state,
    starting: true,
    running: true,
    stopReason: null,
    items: [...state.items, { kind: "user", text, chips }],
  };
}

/** `agent.start` returned: from now on only this session's events count. */
export function adopt(state: AgentViewState, sessionId: string): AgentViewState {
  return { ...state, sessionId, starting: false };
}

/** `agent.start` failed: show why and stop waiting. */
export function failStart(state: AgentViewState, message: string): AgentViewState {
  return {
    ...state,
    starting: false,
    running: false,
    items: [
      ...state.items,
      {
        kind: "error",
        code: "start_failed",
        message,
        requestId: null,
        retryable: false,
      },
    ],
  };
}

function last(items: readonly TranscriptItem[]): TranscriptItem | undefined {
  return items[items.length - 1];
}

function appendAssistant(
  items: readonly TranscriptItem[],
  field: "text" | "thinking",
  delta: string,
): readonly TranscriptItem[] {
  const tail = last(items);
  if (tail?.kind === "assistant" && tail.open) {
    return [...items.slice(0, -1), { ...tail, [field]: tail[field] + delta }];
  }
  return [
    ...items,
    {
      kind: "assistant",
      text: field === "text" ? delta : "",
      thinking: field === "thinking" ? delta : "",
      open: true,
    },
  ];
}

/** A tool call or proposal ends the current assistant paragraph. */
function closeAssistant(items: readonly TranscriptItem[]): readonly TranscriptItem[] {
  const tail = last(items);
  if (tail?.kind === "assistant" && tail.open) {
    return [...items.slice(0, -1), { ...tail, open: false }];
  }
  return items;
}

function str(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function num(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function asEdit(value: unknown): TextEdit | null {
  if (typeof value !== "object" || value === null) return null;
  const v = value as Record<string, unknown>;
  if (
    typeof v["path"] !== "string" ||
    typeof v["from"] !== "number" ||
    typeof v["to"] !== "number" ||
    typeof v["before"] !== "string" ||
    typeof v["after"] !== "string"
  ) {
    return null;
  }
  return {
    path: v["path"],
    from: v["from"],
    to: v["to"],
    before: v["before"],
    after: v["after"],
    explanation: str(v["explanation"]),
  };
}

/** Applies one `agent.*` event. */
export function reduce(
  state: AgentViewState,
  topic: string,
  payload: unknown,
): AgentViewState {
  if (!topic.startsWith("agent.") || typeof payload !== "object" || payload === null) {
    return state;
  }
  const p = payload as Record<string, unknown>;
  const session = str(p["session_id"]);
  if (state.sessionId === null && state.starting && topic === "agent.started") {
    state = { ...state, sessionId: session, starting: false };
  }
  if (session !== state.sessionId) return state;

  switch (topic) {
    case "agent.started":
      return { ...state, provider: str(p["provider"]), model: str(p["model"]) };
    case "agent.text-delta":
      return { ...state, items: appendAssistant(state.items, "text", str(p["text"])) };
    case "agent.thinking-delta":
      return {
        ...state,
        items: appendAssistant(state.items, "thinking", str(p["text"])),
      };
    case "agent.tool-call":
      return {
        ...state,
        items: [
          ...closeAssistant(state.items),
          {
            kind: "tool",
            id: str(p["id"]),
            name: str(p["name"]),
            risk: str(p["risk"]),
            state: "running",
            summary: "",
          },
        ],
      };
    case "agent.tool-result": {
      const id = str(p["id"]);
      return {
        ...state,
        items: state.items.map((item) =>
          item.kind === "tool" && item.id === id
            ? {
                ...item,
                state: p["is_error"] === true ? "error" : "done",
                summary: str(p["summary"]),
              }
            : item,
        ),
      };
    }
    case "agent.approval-requested":
      return {
        ...state,
        items: [
          ...closeAssistant(state.items),
          {
            kind: "approval",
            callId: str(p["call_id"]),
            tool: str(p["tool"]),
            risk: str(p["risk"]),
            scope: str(p["scope"]),
            afterUntrusted: p["after_untrusted_content"] === true,
            answer: "pending",
          },
        ],
      };
    case "agent.proposal":
      return {
        ...state,
        items: [
          ...closeAssistant(state.items),
          {
            kind: "proposal",
            proposalId: str(p["proposal_id"]),
            proposalKind: str(p["kind"], "text_edit"),
            edit: asEdit(p["payload"]),
            state: "pending",
          },
        ],
      };
    case "agent.usage": {
      const total = (p["total"] ?? {}) as Record<string, unknown>;
      const cost = total["est_cost_micro_usd"];
      return {
        ...state,
        usage: {
          input: num(total["input"]),
          output: num(total["output"]),
          cacheRead: num(total["cache_read"]),
          cacheWrite: num(total["cache_write"]),
          costMicroUsd: typeof cost === "number" ? cost : null,
        },
      };
    }
    case "agent.error":
      return {
        ...state,
        items: [
          ...closeAssistant(state.items),
          {
            kind: "error",
            code: str(p["code"], "error"),
            message: str(p["message"], "The request failed."),
            requestId: typeof p["request_id"] === "string" ? p["request_id"] : null,
            retryable: p["retryable"] === true,
          },
        ],
      };
    case "agent.cancelled":
      return {
        ...state,
        items: [...closeAssistant(state.items), { kind: "notice", text: "Stopped." }],
      };
    case "agent.done":
      return {
        ...state,
        running: false,
        stopReason: str(p["stop_reason"]),
        items: closeAssistant(state.items),
      };
    default:
      return state;
  }
}

/** Marks an approval answered. */
export function answerApproval(
  state: AgentViewState,
  callId: string,
  allowed: boolean,
): AgentViewState {
  return {
    ...state,
    items: state.items.map((item) =>
      item.kind === "approval" && item.callId === callId
        ? { ...item, answer: allowed ? "allowed" : "denied" }
        : item,
    ),
  };
}

/** Marks a proposal decided (or found stale when the text moved on). */
export function decideProposal(
  state: AgentViewState,
  proposalId: string,
  next: "accepted" | "rejected" | "stale",
): AgentViewState {
  return {
    ...state,
    items: state.items.map((item) =>
      item.kind === "proposal" && item.proposalId === proposalId
        ? { ...item, state: next }
        : item,
    ),
  };
}

/** The proposal waiting for the user, if any (the Accept button belongs to it). */
export function pendingProposal(
  state: AgentViewState,
): Extract<TranscriptItem, { kind: "proposal" }> | undefined {
  for (let i = state.items.length - 1; i >= 0; i -= 1) {
    const item = state.items[i];
    if (item?.kind === "proposal" && item.state === "pending") return item;
  }
  return undefined;
}

/** `~$0.0123`, always with the tilde: an estimate. */
export function formatCost(micro: number | null): string {
  if (micro === null) return "cost unknown";
  const usd = micro / 1_000_000;
  return `~$${usd < 0.01 ? usd.toFixed(4) : usd.toFixed(2)}`;
}
