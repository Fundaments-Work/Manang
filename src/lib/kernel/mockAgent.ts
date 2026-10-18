/**
 * In-memory `agent` plugin for the browser (`pnpm dev`, verify-ui, vitest).
 *
 * THIS IS A MOCK. No provider is contacted: a deterministic fake provider streams canned text,
 * tool rows and, for the editing commands, an `edit_text` proposal, so the whole Agent UI can be
 * exercised without a key. Every answer says so. The shapes (commands, events, `[code]` errors,
 * redaction, preview) follow `crates/manang-agent`; keep them in step with its `manifest.toml`.
 *
 * Like the real plugin it never stores or returns a key: only whether one was given.
 */

import { estimateTokens } from "$lib/agent/context";
import type { Args, KernelError } from "./client";
import type { MockPlugin, MockPluginFactory } from "./mock";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["agent.start", "Start an AI agent session"],
  ["agent.cancel", "Stop an AI agent session"],
  ["agent.approve", "Answer an AI agent approval request"],
  ["agent.apply-proposal", "Accept an AI proposal"],
  ["agent.reject-proposal", "Reject an AI proposal"],
  ["agent.preview-request", "Preview exactly what would be sent to the AI provider"],
  ["agent.commands", "List the slash commands of the AI assistant"],
  ["agent.sessions", "List AI sessions"],
  ["agent.transcript", "Read an AI session transcript"],
  ["agent.models", "List the models of an AI provider"],
  ["agent.test-connection", "Test the AI provider connection and key"],
  ["agent.status", "Report whether the AI agent is ready"],
  ["agent.consent", "Record consent to send data to an AI provider"],
  ["agent.configure", "Apply AI agent settings"],
  ["agent.key.set", "Save an AI provider API key"],
  ["agent.key.status", "Report whether an AI provider key is stored, and where"],
  ["agent.key.clear", "Remove an AI provider API key"],
  ["agent.list-files", "Agent tool: list the project's text sources"],
  ["agent.read-file", "Agent tool: read a project file"],
  ["agent.search", "Agent tool: search the project"],
  ["agent.outline", "Agent tool: outline of a file or the project"],
  ["agent.diagnostics", "Agent tool: list the problems, optionally with pending edits"],
  ["agent.bib-search", "Agent tool: search the bibliography"],
  ["agent.edit-text", "Agent tool: propose a text edit as search and replace pairs"],
  ["agent.ask-user", "Agent tool: ask the author a question"],
];

const SLASH = [
  { name: "cite", description: "Find citations for the selection", hint: "" },
  { name: "explain", description: "Explain the selection", hint: "" },
  { name: "fix", description: "Fix the problem under the cursor", hint: "" },
  { name: "rewrite", description: "Rewrite the selection for clarity", hint: "" },
  { name: "shorten", description: "Make the selection shorter", hint: "" },
  { name: "translate", description: "Translate the selection", hint: "<language>" },
] as const;

const PRESETS = [
  {
    id: "anthropic",
    name: "Anthropic (Claude)",
    models: ["claude-sonnet-5-5", "claude-haiku-5-5"],
    base_url: "https://api.anthropic.com",
    key_required: true,
    local: false,
    notes: "",
  },
  {
    id: "openai",
    name: "OpenAI",
    models: [],
    base_url: "https://api.openai.com/v1",
    key_required: true,
    local: false,
    notes: "",
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    models: ["deepseek-flash", "deepseek-v4-pro"],
    base_url: "https://api.deepseek.com",
    key_required: true,
    local: false,
    notes: "",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    models: [],
    base_url: "https://openrouter.ai/api/v1",
    key_required: true,
    local: false,
    notes: "",
  },
  {
    id: "groq",
    name: "Groq",
    models: [],
    base_url: "https://api.groq.com/openai/v1",
    key_required: true,
    local: false,
    notes: "",
  },
  {
    id: "ollama",
    name: "Ollama (local)",
    models: [],
    base_url: "http://localhost:11434/v1",
    key_required: false,
    local: true,
    notes: "",
  },
  {
    id: "llama_cpp",
    name: "llama.cpp server (local)",
    models: [],
    base_url: "http://localhost:8080/v1",
    key_required: false,
    local: true,
    notes: "",
  },
  {
    id: "lm_studio",
    name: "LM Studio (local)",
    models: [],
    base_url: "http://localhost:1234/v1",
    key_required: false,
    local: true,
    notes: "",
  },
  {
    id: "custom",
    name: "Custom OpenAI-compatible",
    models: [],
    base_url: null,
    key_required: false,
    local: false,
    notes: "",
  },
] as const;

const PRICES: Readonly<Record<string, readonly [number, number]>> = {
  "deepseek-flash": [300_000, 1_200_000],
  "deepseek-v4-pro": [1_320_000, 3_960_000],
  "claude-sonnet-5-5": [2_000_000, 10_000_000],
};

/** The constant prefix, shortened: the real text lives in `crates/manang-agent/src/prompt.rs`. */
const SYSTEM = "You are the writing assistant inside Manang. (mock system prompt)";

function fail(code: string, message: string): KernelError {
  return { message: `[${code}] ${message}` };
}

interface Diag {
  path: string;
  line: number;
  message: string;
  context?: string;
}

interface Context {
  view?: string;
  active?: { path: string; line?: number };
  selection?: { path: string; text: string; line_from?: number; line_to?: number };
  mentions?: { kind: string; path?: string; text?: string; diagnostics?: Diag[] }[];
  buffers?: Record<string, string>;
  diagnostics?: Diag[];
}

const SECRET = /\b(sk-[A-Za-z0-9_-]{20,}|ghp_[A-Za-z0-9]{20,})\b/g;

function redact(text: string): {
  text: string;
  spans: { kind: string; start: number; end: number; replacement: string }[];
} {
  const spans: { kind: string; start: number; end: number; replacement: string }[] = [];
  const out = text.replace(SECRET, (match, _g, offset: number) => {
    spans.push({
      kind: "secret",
      start: offset,
      end: offset + match.length,
      replacement: "[REDACTED:secret]",
    });
    return "[REDACTED:secret]";
  });
  return { text: out, spans };
}

/** The mock's copy of the built-in command templates. */
function expand(command: string, args: string): string {
  switch (command) {
    case "rewrite":
      return `Rewrite the selected text for clarity and flow. ${args}`.trim();
    case "shorten":
      return `Shorten the selected text. ${args}`.trim();
    case "translate":
      return `Translate the selected text into ${args || "English"}.`;
    case "explain":
      return `Explain the selected text briefly. ${args}`.trim();
    case "fix":
      return `Fix the reported problem with the smallest change. ${args}`.trim();
    case "cite":
      return `Find support in the bibliography for the selected claim. ${args}`.trim();
    default:
      return args;
  }
}

type Settings = {
  enabled: boolean;
  provider: string;
  model: string;
  baseUrl: string;
  offline: boolean;
  includeThinking: boolean;
  contextWindow: number;
};

interface Proposal {
  session: string;
  payload: Record<string, unknown>;
  state: "pending" | "accepted" | "rejected";
}

export const agentPlugin: MockPluginFactory = (emit): MockPlugin => {
  const settings: Settings = {
    enabled: true,
    provider: "anthropic",
    model: "",
    baseUrl: "",
    offline: false,
    includeThinking: false,
    contextWindow: 100_000,
  };
  const keys = new Set<string>();
  const consent = new Set<string>();
  const proposals = new Map<string, Proposal>();
  const timers = new Map<string, ReturnType<typeof setTimeout>[]>();
  const sessions = new Map<
    string,
    {
      id: string;
      title: string;
      provider: string;
      model: string;
      state: string;
      updated_ms: number;
      turns: number;
      input: number;
    }
  >();
  let counter = 0;

  const preset = () => PRESETS.find((p) => p.id === settings.provider) ?? PRESETS[0];
  const modelOf = (): string | null => {
    const p = preset();
    return settings.model !== "" ? settings.model : (p.models[0] ?? null);
  };

  function status() {
    const p = preset();
    const problems: string[] = [];
    if (!settings.enabled) problems.push("disabled");
    if (p.key_required && !keys.has(p.id)) problems.push("key_missing");
    if (!consent.has(p.id)) problems.push("consent_required");
    if (modelOf() === null) problems.push("model_missing");
    const baseUrl = settings.baseUrl !== "" ? settings.baseUrl : (p.base_url ?? "");
    const host = (() => {
      try {
        return new URL(baseUrl).host;
      } catch {
        return baseUrl;
      }
    })();
    const keyPresent = keys.has(p.id);
    return {
      enabled: settings.enabled,
      ready: settings.enabled && problems.length === 0,
      problems,
      provider: p.id,
      provider_name: p.name,
      model: modelOf(),
      base_url: baseUrl,
      engine: "rig",
      offline: settings.offline,
      local: p.local,
      key_required: p.key_required,
      key: {
        present: keyPresent,
        store: "session",
        saved: false,
        notice:
          "Not saved: this is the browser mock, so the key is only remembered for this page.",
      },
      consent: consent.has(p.id),
      consent_text: p.local
        ? `When you press Send, Manang sends your message, the text you select and the parts of your project the assistant reads to the server at ${host}, which is on this machine. Nothing is sent in the background. (Browser mock: nothing is sent at all.)`
        : `When you press Send, Manang sends your message, the text you select and the parts of your project the assistant reads (through its read tools) to ${p.name} at ${host}. Nothing is sent in the background, and you can preview exactly what will be sent. The provider may bill you; your key stays in your system keychain. You can turn the agent off in Settings. (Browser mock: nothing is sent at all.)`,
      context_window: settings.contextWindow,
      include_thinking: settings.includeThinking,
      presets: PRESETS,
      mock: true,
    };
  }

  function prepare(args: Args, sending: boolean) {
    if (!settings.enabled)
      throw fail("disabled", "the AI agent is turned off in Settings");
    const p = preset();
    if (settings.offline) {
      const base = settings.baseUrl !== "" ? settings.baseUrl : (p.base_url ?? "");
      if (!/^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])([:/]|$)/.test(base)) {
        throw fail(
          "offline_blocked",
          "offline mode only allows localhost or a loopback address as the provider address",
        );
      }
    }
    if (sending) {
      if (p.key_required && !keys.has(p.id))
        throw fail(
          "key_missing",
          `no API key is saved for ${p.name}: add one in Settings`,
        );
      if (!consent.has(p.id))
        throw fail(
          "consent_required",
          `agree to send data to ${p.name} first (Settings, Provider and key)`,
        );
    }
    const model = modelOf();
    if (model === null) throw fail("invalid_argument", "choose a model in Settings");
    const context = (args["context"] as Context | undefined) ?? {};
    const command = typeof args["command"] === "string" ? args["command"] : "";
    if (command !== "" && !SLASH.some((c) => c.name === command))
      throw fail("unknown_command", `there is no command \`/${command}\``);
    const typed = typeof args["prompt"] === "string" ? args["prompt"].trim() : "";
    const text = command !== "" ? expand(command, typed) : typed;
    if (text === "" && args["retry"] !== true)
      throw fail("invalid_argument", "write a message first");
    // The parts that travel with the message: the selection and the @-mentions.
    const raw: { id: string; kind: string; label: string; path?: string; text: string }[] =
      [];
    if (context.selection !== undefined) {
      raw.push({
        id: "selection",
        kind: "selection",
        label: "Selection",
        path: context.selection.path,
        text: context.selection.text,
      });
    }
    for (const m of context.mentions ?? []) {
      if (m.kind === "file" && m.path !== undefined)
        raw.push({
          id: `file:${m.path}`,
          kind: "file",
          label: m.path,
          path: m.path,
          text: m.text ?? `(the host reads ${m.path})`,
        });
      if (m.kind === "problems")
        raw.push({
          id: "problems",
          kind: "problems",
          label: "Problems",
          text: (m.diagnostics ?? [])
            .map((d) => `error ${d.path}:${d.line} ${d.message}\n${d.context ?? ""}`)
            .join("\n"),
        });
      if (m.kind === "figure")
        raw.push({
          id: "figure",
          kind: "figure",
          label: "Figure",
          text: "Figure: 3 shapes, 2 connections (mock description)",
        });
    }
    const parts = raw.map((c) => {
      const r = redact(c.text);
      return {
        id: c.id,
        kind: c.kind,
        label: c.label,
        path: c.path,
        tokens: estimateTokens(r.text),
        truncated: false,
        text: r.text,
        original: r.spans.length > 0 ? c.text : undefined,
        redactions: r.spans,
        source: c.text,
      };
    });
    return { model, parts, context, command, text, typed, provider: p };
  }

  function later(session: string, ms: number, fn: () => void): void {
    const handle = setTimeout(fn, ms);
    const list = timers.get(session) ?? [];
    list.push(handle);
    timers.set(session, list);
  }

  /** The edit the mock proposes for a command, or null (the model would only answer). */
  function propose(
    command: string,
    context: Context,
    parts: ReturnType<typeof prepare>["parts"],
    target: string,
  ) {
    if (command === "fix") {
      const d = context.diagnostics?.[0] ?? context.mentions?.[0]?.diagnostics?.[0];
      const body = (d?.context ?? "").trim();
      if (d === undefined || body === "") return null;
      return {
        kind: "text_edit",
        path: d.path,
        note: "Mock fix: appended a marker. A real provider proposes a real change.",
        edits: [
          {
            from: 0,
            to: body.length,
            before: body,
            after: `${body.replace(/\s+$/, "")} (mock fix)`,
          },
        ],
      };
    }
    const selection = parts.find((c) => c.kind === "selection" && c.path !== undefined);
    if (selection?.path === undefined) return null;
    const before = selection.source;
    const words = before.split(/\s+/).filter((w) => w !== "");
    const after =
      command === "shorten"
        ? `${words.slice(0, Math.max(1, Math.ceil(words.length / 2))).join(" ")}.`
        : command === "translate"
          ? `[mock translation to ${target}] ${before}`
          : `${before.replace(/\s+/g, " ").trim()} (mock rewrite)`;
    const buffer = context.buffers?.[selection.path] ?? "";
    const from = Math.max(0, buffer.indexOf(before));
    return {
      kind: "text_edit",
      path: selection.path,
      note: `Mock ${command}: a labelled placeholder edit.`,
      edits: [{ from, to: from + before.length, before, after }],
    };
  }

  function start(args: Args) {
    const { model, parts, context, command, text, typed } = prepare(args, true);
    counter += 1;
    const id =
      typeof args["session_id"] === "string"
        ? args["session_id"]
        : `smock${counter.toString(16)}`;
    const slow = /\bslow\b/i.test(text);
    const asksFirst = /\bdelete\b/i.test(text);
    const wantsVerify = /\bverify\b/i.test(text);
    const p = preset();
    const existing = sessions.get(id);
    sessions.set(id, {
      id,
      title: existing?.title ?? (text || command).slice(0, 60),
      provider: p.id,
      model,
      state: "running",
      updated_ms: Date.now(),
      turns: (existing?.turns ?? 0) + 1,
      input: existing?.input ?? 0,
    });
    const contextTokens = parts.reduce((s, c) => s + c.tokens, 0);
    emit("agent.started", {
      session_id: id,
      provider: p.id,
      model,
      effort: command !== "" ? "low" : "high",
      context_tokens: contextTokens,
      window: settings.contextWindow,
      parts: parts.map((c) => ({
        id: c.id,
        kind: c.kind,
        label: c.label,
        tokens: c.tokens,
      })),
    });
    let at = 5;
    // Reasoning is a signal, not text, unless the developer flag is on.
    later(id, at, () =>
      emit("agent.text", {
        session_id: id,
        thinking: true,
        text: settings.includeThinking ? "Mock reasoning: looking at the context. " : "",
      }),
    );
    at += 10;
    const proposal =
      command === "" || command === "explain" || command === "cite"
        ? null
        : propose(command, context, parts, typed || "English");
    const reads = proposal !== null || /\bread\b/i.test(text);
    const answer = asksFirst
      ? ""
      : command === "explain"
        ? "Mock explanation: this passage states a result and then qualifies it. (No provider was contacted.)"
        : proposal !== null
          ? "Mock answer (no provider was contacted). I propose this change:"
          : `Mock answer (no provider was contacted). You asked: ${text || command}. Select some text and type /rewrite or /fix to see a proposal.`;
    const row = (
      tid: string,
      name: string,
      toolArgs: Record<string, unknown>,
      result: string,
      extra: Record<string, unknown> = {},
    ) => {
      later(id, (at += 8), () =>
        emit("agent.tool", {
          session_id: id,
          id: tid,
          name,
          risk: name === "edit_text" ? "propose" : "read",
          status: "running",
          args: toolArgs,
          summary: "",
        }),
      );
      later(id, (at += 8), () =>
        emit("agent.tool", {
          session_id: id,
          id: tid,
          name,
          risk: name === "edit_text" ? "propose" : "read",
          status: "done",
          args: toolArgs,
          summary: result,
          ...extra,
        }),
      );
    };
    if (reads) {
      const path = context.active?.path ?? "main.typ";
      row("t1", "read_file", { path }, `${path} lines 1-40 of 40`);
      row("t2", "search", { query: "Landauer" }, "3 matches in 2 files.");
    }
    if (asksFirst) {
      row(
        "ask1",
        "ask_user",
        { question: "Delete that section?" },
        "Question shown to the author.",
        { ask: { question: "Delete that section?", choices: ["Yes, delete it", "No"] } },
      );
    }
    if (wantsVerify) {
      later(id, (at += 8), () =>
        emit("agent.approval", {
          session_id: id,
          call_id: `${id}/v1`,
          tool_call_id: "v1",
          tool: "verify_citation",
          risk: "network",
          scope: "network:api.crossref.org",
          host: "api.crossref.org",
          args: { doi: "10.1000/mock" },
          after_untrusted_content: false,
        }),
      );
    }
    const pieces = answer === "" ? [] : (answer.match(/.{1,12}/g) ?? [answer]);
    const gap = slow ? 400 : 12;
    for (const piece of pieces) {
      at += gap;
      later(id, at, () => emit("agent.text", { session_id: id, text: piece }));
    }
    if (proposal !== null) {
      at += 20;
      later(id, at, () =>
        emit("agent.tool", {
          session_id: id,
          id: "e1",
          name: "edit_text",
          risk: "propose",
          status: "running",
          args: { path: proposal.path },
          summary: "",
        }),
      );
      at += 10;
      later(id, at, () => {
        counter += 1;
        const proposalId = `pmock${counter.toString(16)}`;
        proposals.set(proposalId, { session: id, payload: proposal, state: "pending" });
        emit("agent.tool", {
          session_id: id,
          id: "e1",
          name: "edit_text",
          risk: "propose",
          status: "done",
          args: { path: proposal.path },
          summary: `Proposal ${proposalId} queued`,
          proposal_id: proposalId,
        });
        emit("agent.proposal", {
          session_id: id,
          proposal_id: proposalId,
          kind: "text_edit",
          payload: proposal,
        });
      });
    }
    at += 20;
    later(id, at, () => {
      const s = sessions.get(id);
      const input = contextTokens + estimateTokens(text) + 1900 + (s?.input ?? 0);
      const output = estimateTokens(answer) + 20;
      const price = PRICES[model];
      const cost =
        price === undefined
          ? null
          : Math.ceil((input * price[0] + output * price[1]) / 1_000_000);
      if (s !== undefined) {
        s.input = input;
        s.state = "idle";
      }
      emit("agent.usage", {
        session_id: id,
        model,
        input,
        output,
        cache_read: 0,
        cache_write: 0,
        est_cost_micro_usd: cost,
        requests: 1,
        cache_hit_percent: 0,
        fill: input,
        window: settings.contextWindow,
        compacted: null,
        total: {
          input,
          output,
          cache_read: 0,
          cache_write: 0,
          est_cost_micro_usd: cost,
        },
      });
      timers.delete(id);
      emit("agent.done", {
        session_id: id,
        stop_reason: asksFirst ? "ask_user" : "end_turn",
        steps: 12,
      });
    });
    return { session_id: id };
  }

  const handlers: Record<string, (args: Args) => unknown> = {
    "agent.configure": (args) => {
      const values = (args["values"] ?? {}) as Record<string, unknown>;
      if (typeof values["agent.enabled"] === "boolean")
        settings.enabled = values["agent.enabled"];
      if (typeof values["agent.provider"] === "string") {
        if (!PRESETS.some((p) => p.id === values["agent.provider"]))
          throw fail(
            "invalid_argument",
            `unknown provider \`${String(values["agent.provider"])}\``,
          );
        settings.provider = values["agent.provider"];
      }
      if (typeof values["agent.model"] === "string")
        settings.model = values["agent.model"].trim();
      if (typeof values["agent.base-url"] === "string")
        settings.baseUrl = values["agent.base-url"].trim();
      if (typeof values["agent.offline"] === "boolean")
        settings.offline = values["agent.offline"];
      if (typeof values["agent.include-thinking"] === "boolean")
        settings.includeThinking = values["agent.include-thinking"];
      if (typeof values["agent.context-window"] === "number")
        settings.contextWindow = values["agent.context-window"];
      return { applied: Object.keys(values).length };
    },
    "agent.status": () => status(),
    "agent.commands": () => ({
      commands: SLASH.map((c) => ({ ...c, source: "builtin" })),
    }),
    "agent.consent": (args) => {
      const provider = String(args["provider"]);
      if (!PRESETS.some((p) => p.id === provider))
        throw fail("invalid_argument", "unknown provider");
      if (args["granted"] === false) consent.delete(provider);
      else consent.add(provider);
      return { provider, consent: args["granted"] !== false };
    },
    "agent.key.set": (args) => {
      const key = typeof args["key"] === "string" ? args["key"].trim() : "";
      if (key === "") throw fail("invalid_argument", "the key is empty");
      keys.add(String(args["provider"]));
      // The key itself is dropped here: only its presence is kept.
      return {
        store: "session",
        saved: false,
        notice:
          "Not saved: this is the browser mock, so the key is only remembered for this page.",
      };
    },
    "agent.key.status": (args) => ({
      present: keys.has(String(args["provider"])),
      store: "session",
      saved: false,
      notice:
        "Not saved: this is the browser mock, so the key is only remembered for this page.",
    }),
    "agent.key.clear": (args) => ({ cleared: keys.delete(String(args["provider"])) }),
    "agent.models": (args) => {
      const p = PRESETS.find((x) => x.id === (args["provider"] ?? settings.provider));
      if (p === undefined) throw fail("invalid_argument", "unknown provider");
      return {
        provider: p.id,
        models: [...p.models],
        default: p.models[0] ?? null,
        fetch_supported: false,
        note: p.notes,
      };
    },
    "agent.test-connection": () => {
      if (!settings.enabled)
        throw fail("disabled", "the AI agent is turned off in Settings");
      const p = preset();
      if (p.key_required && !keys.has(p.id))
        return {
          ok: false,
          code: "key_missing",
          message: `no API key is saved for ${p.name}: add one in Settings`,
        };
      return { ok: true, model: modelOf(), latency_ms: 1, mock: true };
    },
    "agent.preview-request": (args) => {
      const { model, parts, text, provider } = prepare(args, false);
      const first =
        parts.map((c) => `<${c.kind}>\n${c.text}\n</${c.kind}>`).join("\n") +
        (parts.length > 0 ? "\n\n" : "") +
        text;
      const body = JSON.stringify(
        {
          model,
          messages: [
            { role: "system", content: SYSTEM },
            { role: "user", content: first },
          ],
          stream: true,
        },
        null,
        2,
      );
      const price = PRICES[model];
      const input = estimateTokens(body) + 1900;
      return {
        provider: provider.id,
        model,
        url: `${settings.baseUrl !== "" ? settings.baseUrl : (provider.base_url ?? "")}/chat/completions`,
        headers: [
          ["authorization", "***"],
          ["content-type", "application/json"],
        ],
        body,
        system: SYSTEM,
        tools: [
          { name: "edit_text", risk: "propose" },
          { name: "read_file", risk: "read" },
        ],
        parts: parts.map(({ source: _source, ...c }) => c),
        redaction_count: parts.reduce((s, c) => s + c.redactions.length, 0),
        tokens: { context: parts.reduce((s, c) => s + c.tokens, 0), request: input },
        cost: {
          est_input_micro_usd:
            price === undefined ? null : Math.ceil((input * price[0]) / 1_000_000),
          estimate: true,
          note: "An estimate of the input only; the provider's bill is the truth.",
        },
        mock: true,
      };
    },
    "agent.start": start,
    "agent.cancel": (args) => {
      const id = String(args["session_id"]);
      const list = timers.get(id);
      if (!sessions.has(id)) throw fail("not_found", "no such session");
      if (list === undefined) return { cancelled: false };
      for (const t of list) clearTimeout(t);
      timers.delete(id);
      const s = sessions.get(id);
      if (s !== undefined) s.state = "idle";
      emit("agent.done", { session_id: id, stop_reason: "cancelled", cancelled: true });
      return { cancelled: true };
    },
    "agent.approve": () => ({ ok: true }),
    "agent.apply-proposal": (args) => decide(String(args["proposal_id"]), true),
    "agent.reject-proposal": (args) => decide(String(args["proposal_id"]), false),
    "agent.sessions": () => ({
      sessions: [...sessions.values()]
        .sort((a, b) => b.updated_ms - a.updated_ms)
        .map((s) => ({ version: 1, created_ms: s.updated_ms, ...s })),
    }),
    "agent.transcript": (args) => {
      if (!sessions.has(String(args["session_id"])))
        throw fail("not_found", "no such session");
      return { session_id: String(args["session_id"]), messages: [] };
    },
  };

  function decide(id: string, accept: boolean) {
    const p = proposals.get(id);
    if (p === undefined) throw fail("not_found", "no such proposal");
    if (p.state !== "pending")
      throw fail("already_decided", "this proposal was already accepted or rejected");
    p.state = accept ? "accepted" : "rejected";
    return accept
      ? { proposal_id: id, kind: "text_edit", edit: p.payload }
      : { proposal_id: id, rejected: true };
  }

  return {
    name: "agent",
    commands: COMMANDS,
    handlers,
    version: "0.1.0",
  };
};
