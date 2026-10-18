/**
 * The Agent view's transcript as a pure reducer over `agent.*` events.
 *
 * Events carry a `session_id`; the reducer ignores any other session, so a late event of a
 * stopped conversation never lands in the new one. State is immutable.
 *
 * Seven kinds of event drive it: `text` (answer deltas, or a `thinking` signal), `tool` (a call
 * and its result are one updating row), `approval`, `proposal`, `usage` (once per message),
 * `error` and `done` (cancellation is a field), plus `started`, which only names the session.
 */

import type { TextEdit } from "./types";

export type ItemState = "running" | "done" | "error";

/** One tool call as a row; consecutive rows fold into one group in the transcript. */
export interface ToolRow {
  readonly id: string;
  readonly name: string;
  readonly risk: string;
  readonly status: ItemState;
  readonly args: Readonly<Record<string, unknown>>;
  readonly summary: string;
}

export type TranscriptItem =
  | { readonly kind: "user"; readonly text: string; readonly context: string }
  | { readonly kind: "assistant"; readonly text: string; readonly open: boolean }
  | { readonly kind: "tools"; readonly rows: readonly ToolRow[] }
  | {
      readonly kind: "ask";
      readonly question: string;
      readonly choices: readonly string[];
      readonly answer: string | null;
    }
  | {
      readonly kind: "approval";
      readonly callId: string;
      readonly tool: string;
      readonly host: string | null;
      readonly scope: string;
      readonly afterUntrusted: boolean;
      readonly answer: "pending" | "allowed" | "denied";
    }
  | {
      readonly kind: "proposal";
      readonly proposalId: string;
      readonly edit: TextEdit | null;
      readonly state: "pending" | "accepted" | "rejected" | "stale" | "undone";
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
  /** Tokens of the whole session so far (not cached input, output). */
  readonly input: number;
  readonly output: number;
  readonly cacheRead: number;
  readonly cacheWrite: number;
  /** Micro-USD; null when the model has no price. Always an estimate. */
  readonly costMicroUsd: number | null;
  /** Prompt tokens of the last request, and the working window they are measured against. */
  readonly fill: number;
  readonly window: number;
  /** Share of the last message's prompt that came from the provider's cache. */
  readonly cacheHitPercent: number;
}

export interface AgentViewState {
  readonly sessionId: string | null;
  /** `agent.start` was sent and its first event has not arrived: adopt that session. */
  readonly starting: boolean;
  readonly running: boolean;
  /** The model is reasoning (shown as a quiet "Thinking" line, never as text). */
  readonly thinking: boolean;
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
  fill: 0,
  window: 0,
  cacheHitPercent: 0,
};

export function initialState(): AgentViewState {
  return {
    sessionId: null,
    starting: false,
    running: false,
    thinking: false,
    items: [],
    usage: EMPTY_USAGE,
    provider: "",
    model: "",
    stopReason: null,
  };
}

/** The author sent a message: show it and wait for the session. */
export function begin(
  state: AgentViewState,
  text: string,
  context: string,
): AgentViewState {
  return {
    ...state,
    starting: true,
    running: true,
    thinking: false,
    stopReason: null,
    items: [...state.items, { kind: "user", text, context }],
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

function appendAnswer(items: readonly TranscriptItem[], delta: string) {
  const tail = last(items);
  if (tail?.kind === "assistant" && tail.open) {
    return [...items.slice(0, -1), { ...tail, text: tail.text + delta }];
  }
  return [...items, { kind: "assistant" as const, text: delta, open: true }];
}

/** A tool call, approval or proposal ends the current answer paragraph. */
function closeAnswer(items: readonly TranscriptItem[]): readonly TranscriptItem[] {
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
  if (typeof v["path"] !== "string" || !Array.isArray(v["edits"])) return null;
  const edits = [];
  for (const raw of v["edits"] as unknown[]) {
    if (typeof raw !== "object" || raw === null) return null;
    const e = raw as Record<string, unknown>;
    if (
      typeof e["from"] !== "number" ||
      typeof e["to"] !== "number" ||
      typeof e["before"] !== "string" ||
      typeof e["after"] !== "string"
    ) {
      return null;
    }
    edits.push({ from: e["from"], to: e["to"], before: e["before"], after: e["after"] });
  }
  if (edits.length === 0) return null;
  return { path: v["path"], note: str(v["note"]), edits };
}

function asArgs(value: unknown): Readonly<Record<string, unknown>> {
  return typeof value === "object" && value !== null
    ? (value as Record<string, unknown>)
    : {};
}

/** Puts a tool row into the trailing group (or starts one); a row with the same id is updated. */
function upsertRow(
  items: readonly TranscriptItem[],
  row: ToolRow,
): readonly TranscriptItem[] {
  const tail = last(items);
  if (tail?.kind === "tools") {
    const at = tail.rows.findIndex((r) => r.id === row.id);
    const rows =
      at >= 0
        ? tail.rows.map((r, i) => (i === at ? row : r))
        : [...tail.rows, row];
    return [...items.slice(0, -1), { kind: "tools", rows }];
  }
  return [...closeAnswer(items), { kind: "tools", rows: [row] }];
}

function stopNotice(reason: string, p: Record<string, unknown>): string | null {
  switch (reason) {
    case "iteration_cap":
      return `Stopped after ${num(p["steps"]) || 12} steps. Say continue to go on.`;
    case "context_full":
      return "This chat has grown too long to continue. Start a new chat.";
    case "max_tokens":
      return "The answer was cut at the length limit. Say continue to go on.";
    default:
      return null;
  }
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
      return {
        ...state,
        provider: str(p["provider"]),
        model: str(p["model"]),
        usage: { ...state.usage, window: num(p["window"]) || state.usage.window },
      };
    case "agent.text":
      if (p["thinking"] === true) return { ...state, thinking: true };
      return { ...state, thinking: false, items: appendAnswer(state.items, str(p["text"])) };
    case "agent.tool": {
      const name = str(p["name"]);
      const status = str(p["status"]) as ItemState;
      const ask = p["ask"];
      if (typeof ask === "object" && ask !== null && status !== "running") {
        const a = ask as Record<string, unknown>;
        return {
          ...state,
          thinking: false,
          items: [
            ...closeAnswer(state.items),
            {
              kind: "ask",
              question: str(a["question"]),
              choices: Array.isArray(a["choices"])
                ? (a["choices"] as unknown[]).filter((c): c is string => typeof c === "string")
                : [],
              answer: null,
            },
          ],
        };
      }
      // A queued proposal shows as its own row and ask_user as a question; only an edit the
      // host refused is a (visible) failed tool row.
      if (name === "ask_user" || (name === "edit_text" && status !== "error"))
        return { ...state, thinking: false };
      return {
        ...state,
        thinking: false,
        items: upsertRow(state.items, {
          id: str(p["id"]),
          name,
          risk: str(p["risk"]),
          status,
          args: asArgs(p["args"]),
          summary: str(p["summary"]),
        }),
      };
    }
    case "agent.approval":
      return {
        ...state,
        thinking: false,
        items: [
          ...closeAnswer(state.items),
          {
            kind: "approval",
            callId: str(p["call_id"]),
            tool: str(p["tool"]),
            host: typeof p["host"] === "string" ? p["host"] : null,
            scope: str(p["scope"]),
            afterUntrusted: p["after_untrusted_content"] === true,
            answer: "pending",
          },
        ],
      };
    case "agent.proposal":
      return {
        ...state,
        thinking: false,
        items: [
          ...closeAnswer(state.items),
          {
            kind: "proposal",
            proposalId: str(p["proposal_id"]),
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
          fill: num(p["fill"]),
          window: num(p["window"]) || state.usage.window,
          cacheHitPercent: num(p["cache_hit_percent"]),
        },
      };
    }
    case "agent.error":
      return {
        ...state,
        thinking: false,
        items: [
          ...closeAnswer(state.items),
          {
            kind: "error",
            code: str(p["code"], "error"),
            message: str(p["message"], "The request failed."),
            requestId: typeof p["request_id"] === "string" ? p["request_id"] : null,
            retryable: p["retryable"] === true,
          },
        ],
      };
    case "agent.done": {
      const reason = str(p["stop_reason"]);
      const notice =
        p["cancelled"] === true ? "Stopped." : stopNotice(reason, p);
      const items = closeAnswer(state.items);
      return {
        ...state,
        running: false,
        thinking: false,
        stopReason: reason,
        items: notice === null ? items : [...items, { kind: "notice", text: notice }],
      };
    }
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

/** Marks the question at `index` answered with `answer` (a choice or typed text). */
export function answerAsk(
  state: AgentViewState,
  index: number,
  answer: string,
): AgentViewState {
  return {
    ...state,
    items: state.items.map((item, i) =>
      item.kind === "ask" && i === index ? { ...item, answer } : item,
    ),
  };
}

/** Marks a proposal decided (or found stale when the text moved on). */
export function decideProposal(
  state: AgentViewState,
  proposalId: string,
  next: "accepted" | "rejected" | "stale" | "undone",
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

/** The proposal waiting for the author, if any (the Accept button belongs to it). */
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

/** The working window's fill as a whole percent (0 when unknown). */
export function fillPercent(usage: Usage): number {
  return usage.window === 0
    ? 0
    : Math.min(100, Math.round((usage.fill * 100) / usage.window));
}
