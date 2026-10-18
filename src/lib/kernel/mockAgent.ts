/**
 * In-memory `agent` plugin for the browser (`pnpm dev`, verify-ui, vitest).
 *
 * THIS IS A MOCK. No provider is contacted: a deterministic fake provider streams canned
 * text, reasoning, usage and, for the editing features, a `doc_propose_edit` proposal, so the
 * whole Agent UI can be exercised without a key. Every answer says so. The shapes (commands,
 * events, `[code]` errors, redaction, preview) follow `crates/manang-agent`; keep them in step
 * with its `manifest.toml`.
 *
 * Like the real plugin it never stores or returns a key: only whether one was given.
 */

import { estimateTokens } from "$lib/agent/chips";
import type { Args, KernelError } from "./client";
import type { MockPlugin, MockPluginFactory } from "./mock";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["agent.start", "Start an AI agent session"],
  ["agent.cancel", "Stop an AI agent session"],
  ["agent.approve", "Answer an AI agent approval request"],
  ["agent.apply-proposal", "Accept an AI proposal"],
  ["agent.reject-proposal", "Reject an AI proposal"],
  ["agent.preview-request", "Preview exactly what would be sent to the AI provider"],
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
  ["agent.doc-read", "Agent tool: read part of a project file"],
  ["agent.doc-diagnostics", "Agent tool: list the editor's problems"],
  ["agent.doc-outline", "Agent tool: outline of a document"],
  ["agent.doc-propose-edit", "Agent tool: propose a text edit"],
  ["agent.project-search", "Agent tool: search the project"],
];

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

function fail(code: string, message: string): KernelError {
  return { message: `[${code}] ${message}` };
}

interface Chip {
  id?: string;
  kind: string;
  label?: string;
  path?: string;
  range?: [number, number];
  text?: string;
  diagnostics?: { path: string; line: number; message: string; context?: string }[];
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

function chipText(chip: Chip): string {
  if (chip.text !== undefined) return chip.text;
  if (chip.kind === "diagnostics") {
    return (chip.diagnostics ?? [])
      .map((d) => `error at ${d.path}:${d.line}: ${d.message}\n${d.context ?? ""}`)
      .join("\n");
  }
  if (chip.kind === "figure")
    return "Figure: 3 shapes, 2 connections (mock description)";
  if (chip.kind === "outline") return "Introduction (line 1)\n  Results (line 20)";
  if (chip.kind === "bibliography-keys")
    return "landauer1961, berut2012, lloyd2008, nielsen2010";
  return "";
}

type Settings = {
  enabled: boolean;
  provider: string;
  model: string;
  baseUrl: string;
  offline: boolean;
  includeThinking: boolean;
  maxContext: number;
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
    maxContext: 32_000,
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
      feature: string;
      provider: string;
      model: string;
      state: string;
      updated_ms: number;
      turns: number;
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
        ? `When you press Send, Manang sends the text you attach as context and your message to the server at ${host}, which is on this machine. Nothing is sent in the background. (Browser mock: nothing is sent at all.)`
        : `When you press Send, Manang sends the text you attach as context (the selection, file, problems or figure you choose in the context tray) and your message to ${p.name} at ${host}. Nothing is sent in the background, and you can preview exactly what will be sent. The provider may bill you; your key stays in your system keychain. You can turn the agent off in Settings. (Browser mock: nothing is sent at all.)`,
      max_context_tokens: settings.maxContext,
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
    const chips = ((args["context"] as Chip[] | undefined) ?? []).map((c, i) => {
      const original = chipText(c);
      const r = redact(original);
      return {
        id: c.id ?? `${c.kind}-${i}`,
        kind: c.kind,
        label: c.label ?? c.kind,
        path: c.path,
        range: c.range,
        tokens: estimateTokens(r.text),
        truncated: false,
        text: r.text,
        original: r.spans.length > 0 ? original : undefined,
        redactions: r.spans,
        source: original,
      };
    });
    const context = chips.reduce((s, c) => s + c.tokens, 0);
    if (context > settings.maxContext) {
      throw fail(
        "context_too_large",
        `the attached context is about ${context} tokens; the limit is ${settings.maxContext}.`,
      );
    }
    return { model, chips, context, provider: p };
  }

  function later(session: string, ms: number, fn: () => void): void {
    const handle = setTimeout(fn, ms);
    const list = timers.get(session) ?? [];
    list.push(handle);
    timers.set(session, list);
  }

  function propose(
    feature: string,
    chips: ReturnType<typeof prepare>["chips"],
    target: string,
  ) {
    const selection = chips.find((c) => c.kind === "selection" && c.path !== undefined);
    if (feature === "fix-diagnostic") {
      const d = chips.find((c) => c.kind === "diagnostics");
      const context = (d?.source ?? "").split("\n").slice(1).join("\n").trim();
      if (context === "") return null;
      const file =
        chips
          .find((c) => c.kind === "diagnostics")
          ?.source.match(/ at ([^:]+):/)?.[1] ?? "";
      return {
        path: file,
        from: 0,
        to: context.length,
        before: context,
        after: `${context.replace(/\s+$/, "")} (mock fix)`,
        explanation:
          "Mock fix: appended a marker. A real provider proposes a real change.",
      };
    }
    if (
      selection === undefined ||
      selection.path === undefined ||
      selection.range === undefined
    )
      return null;
    const before = selection.source;
    const words = before.split(/\s+/).filter((w) => w !== "");
    const after =
      feature === "shorten"
        ? `${words.slice(0, Math.max(1, Math.ceil(words.length / 2))).join(" ")}.`
        : feature === "translate"
          ? `[mock translation to ${target}] ${before}`
          : `${before.replace(/\s+/g, " ").trim()} (mock rewrite)`;
    return {
      path: selection.path,
      from: selection.range[0],
      to: selection.range[1],
      before,
      after,
      explanation: `Mock ${feature}: a labelled placeholder edit.`,
    };
  }

  function start(args: Args) {
    const { model, chips, context } = prepare(args, true);
    const feature = String(args["feature"]);
    const prompt = typeof args["prompt"] === "string" ? args["prompt"] : "";
    const options = (args["options"] as { target_language?: string } | undefined) ?? {};
    counter += 1;
    const id =
      typeof args["session_id"] === "string"
        ? args["session_id"]
        : `smock${counter.toString(16)}`;
    const slow = /\bslow\b/i.test(prompt);
    const p = preset();
    sessions.set(id, {
      id,
      title: (prompt || feature).slice(0, 60),
      feature,
      provider: p.id,
      model,
      state: "running",
      updated_ms: Date.now(),
      turns: 1,
    });
    emit("agent.started", {
      session_id: id,
      provider: p.id,
      model,
      feature,
      context_tokens: context,
      chips: chips.map((c) => ({
        id: c.id,
        kind: c.kind,
        label: c.label,
        tokens: c.tokens,
      })),
    });
    let at = 5;
    if (settings.includeThinking) {
      later(id, at, () =>
        emit("agent.thinking-delta", {
          session_id: id,
          text: "Mock reasoning: looking at the attached context. ",
        }),
      );
      at += 10;
    }
    const proposal =
      feature === "chat" || feature === "explain"
        ? null
        : propose(feature, chips, options.target_language ?? "English");
    const answer =
      feature === "explain"
        ? "Mock explanation: this passage states a result and then qualifies it. (No provider was contacted.)"
        : proposal !== null
          ? "Mock answer (no provider was contacted). I propose this change:"
          : `Mock answer (no provider was contacted). You asked: ${prompt || feature}. Attach a selection or a problem and choose Rewrite or Fix to see a proposal.`;
    const pieces = answer.match(/.{1,12}/g) ?? [answer];
    const gap = slow ? 400 : 12;
    for (const piece of pieces) {
      at += gap;
      later(id, at, () => emit("agent.text-delta", { session_id: id, text: piece }));
    }
    if (proposal !== null) {
      at += 20;
      later(id, at, () =>
        emit("agent.tool-call", {
          session_id: id,
          id: "t1",
          name: "doc_propose_edit",
          risk: "propose",
          args: proposal,
        }),
      );
      at += 10;
      later(id, at, () => {
        counter += 1;
        const proposalId = `pmock${counter.toString(16)}`;
        proposals.set(proposalId, {
          session: id,
          payload: { kind: "text_edit", ...proposal },
          state: "pending",
        });
        emit("agent.tool-result", {
          session_id: id,
          id: "t1",
          is_error: false,
          summary: "proposal recorded",
        });
        emit("agent.proposal", {
          session_id: id,
          proposal_id: proposalId,
          kind: "text_edit",
          payload: { kind: "text_edit", ...proposal },
        });
      });
    }
    at += 20;
    later(id, at, () => {
      const input =
        chips.reduce((s, c) => s + c.tokens, 0) + estimateTokens(prompt) + 120;
      const output = estimateTokens(answer) + 20;
      const price = PRICES[model];
      const cost =
        price === undefined
          ? null
          : Math.ceil((input * price[0] + output * price[1]) / 1_000_000);
      emit("agent.usage", {
        session_id: id,
        model,
        input,
        output,
        cache_read: 0,
        cache_write: 0,
        est_cost_micro_usd: cost,
        total: {
          input,
          output,
          cache_read: 0,
          cache_write: 0,
          est_cost_micro_usd: cost,
        },
      });
      const s = sessions.get(id);
      if (s !== undefined) s.state = "idle";
      timers.delete(id);
      emit("agent.done", { session_id: id, stop_reason: "end_turn" });
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
      if (typeof values["agent.max-context-tokens"] === "number")
        settings.maxContext = values["agent.max-context-tokens"];
      return { applied: Object.keys(values).length };
    },
    "agent.status": () => status(),
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
      const { model, chips, context, provider } = prepare(args, false);
      const prompt = typeof args["prompt"] === "string" ? args["prompt"] : "";
      const system =
        "You are the writing and figure assistant inside Manang. (mock system prompt)";
      const user = `<manang_context>\n${chips.map((c) => `<chip kind="${c.kind}">\n${c.text}\n</chip>`).join("\n")}\n</manang_context>\n\n<user_request>\n${prompt || String(args["feature"])}\n</user_request>`;
      const body = JSON.stringify(
        {
          model,
          messages: [
            { role: "system", content: system },
            { role: "user", content: user },
          ],
          stream: true,
        },
        null,
        2,
      );
      const price = PRICES[model];
      const input = estimateTokens(body);
      return {
        provider: provider.id,
        model,
        url: `${settings.baseUrl !== "" ? settings.baseUrl : (provider.base_url ?? "")}/chat/completions`,
        headers: [
          ["authorization", "***"],
          ["content-type", "application/json"],
        ],
        body,
        system,
        tools: [
          { name: "doc_propose_edit", risk: "propose" },
          { name: "doc_read", risk: "read" },
        ],
        chips: chips.map(({ source: _source, ...c }) => c),
        redaction_count: chips.reduce((s, c) => s + c.redactions.length, 0),
        tokens: { context, request: input },
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
      emit("agent.cancelled", { session_id: id });
      emit("agent.done", { session_id: id, stop_reason: "cancelled" });
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
