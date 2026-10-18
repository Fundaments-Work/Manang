import { describe, expect, it } from "vitest";

import {
  contextLine,
  estimateTokens,
  mentionLabel,
  parseSlash,
  slashMatches,
  tokenLabel,
  wordCount,
} from "./context";
import {
  describeEdit,
  diffSegments,
  locateEdit,
  locateHunk,
  minimalChange,
  sizeOf,
} from "./proposal";
import {
  adopt,
  answerApproval,
  answerAsk,
  begin,
  decideProposal,
  fillPercent,
  formatCost,
  initialState,
  pendingProposal,
  reduce,
} from "./reducer";
import { groupLine, rowLine } from "./toolLines";
import type { SlashCommand, TextEdit } from "./types";

const COMMANDS: readonly SlashCommand[] = [
  { name: "fix", description: "Fix", hint: "", source: "builtin" },
  { name: "rewrite", description: "Rewrite", hint: "", source: "builtin" },
  { name: "translate", description: "Translate", hint: "<language>", source: "builtin" },
];

describe("context and the composer", () => {
  it("uses about 3.5 characters per token, rounded up, like the host", () => {
    expect(estimateTokens("")).toBe(0);
    expect(estimateTokens("abcdefg")).toBe(2);
    expect(estimateTokens("a".repeat(35))).toBe(10);
    expect(estimateTokens("😀😀😀😀😀😀😀")).toBe(2);
    expect(tokenLabel(950)).toBe("950");
    expect(tokenLabel(1234)).toBe("1.2k");
  });

  it("counts words", () => {
    expect(wordCount("")).toBe(0);
    expect(wordCount("  a  b\nc ")).toBe(3);
  });

  it("shows one quiet line: the file, the selection, the mentions", () => {
    expect(contextLine({ mentions: [] })).toBe("");
    expect(
      contextLine({
        path: "main.typ",
        selectionWords: 38,
        mentions: [{ kind: "file", path: "refs.bib" }, { kind: "problems", diagnostics: [] }],
      }),
    ).toBe("main.typ, selection (38 words), @refs.bib, @problems");
    expect(contextLine({ path: "a.md", selectionWords: 1, mentions: [] })).toBe(
      "a.md, selection (1 word)",
    );
    expect(mentionLabel({ kind: "figure", path: "f.figure" })).toBe("@figure f.figure");
  });

  it("reads a slash command and its arguments, and leaves other text alone", () => {
    expect(parseSlash("/translate German", COMMANDS)).toEqual({
      command: "translate",
      prompt: "German",
    });
    expect(parseSlash("/fix", COMMANDS)).toEqual({ command: "fix", prompt: "" });
    expect(parseSlash("/rewrite more formal\nplease", COMMANDS)).toEqual({
      command: "rewrite",
      prompt: "more formal\nplease",
    });
    expect(parseSlash("/nonsense go", COMMANDS)).toEqual({
      command: null,
      prompt: "/nonsense go",
    });
    expect(parseSlash("  hello ", COMMANDS)).toEqual({ command: null, prompt: "hello" });
  });

  it("offers the commands that fit while the name is being typed", () => {
    expect(slashMatches("/", COMMANDS).map((c) => c.name)).toEqual([
      "fix",
      "rewrite",
      "translate",
    ]);
    expect(slashMatches("/re", COMMANDS).map((c) => c.name)).toEqual(["rewrite"]);
    expect(slashMatches("/fix now", COMMANDS)).toEqual([]);
    expect(slashMatches("hello", COMMANDS)).toEqual([]);
  });
});

describe("diff from a proposal", () => {
  it("shows only what changed between the unchanged ends", () => {
    expect(diffSegments("the quick fox", "the slow fox")).toEqual([
      { kind: "same", text: "the " },
      { kind: "removed", text: "quick" },
      { kind: "inserted", text: "slow" },
      { kind: "same", text: " fox" },
    ]);
    expect(diffSegments("same", "same")).toEqual([{ kind: "same", text: "same" }]);
    expect(diffSegments("", "new")).toEqual([{ kind: "inserted", text: "new" }]);
  });

  it("never splits a surrogate pair", () => {
    const segs = diffSegments("a😀b", "a😁b");
    expect(segs.map((s) => s.text).join("")).toContain("😀");
    expect(segs.find((s) => s.kind === "removed")?.text).toBe("😀");
  });

  const hunk = { from: 10, to: 23, before: "the quick fox", after: "the slow fox" };
  const edit: TextEdit = { path: "a.typ", note: "", edits: [hunk] };

  it("finds the change in editor coordinates and describes the edit", () => {
    expect(minimalChange(hunk, { from: 10, to: 23 })).toEqual({
      from: 14,
      to: 19,
      insert: "slow",
    });
    expect(describeEdit(edit)).toBe("Edit to a.typ, 1 change");
    expect(describeEdit({ ...edit, edits: [hunk, hunk] })).toBe("Edit to a.typ, 2 changes");
    expect(sizeOf(edit)).toEqual({ removed: 13, added: 12 });
  });

  it("locates a hunk, follows moved text and calls changed text stale", () => {
    const h = { from: 4, to: 9, before: "hello", after: "hi" };
    expect(locateHunk("say hello there", h)).toEqual({ from: 4, to: 9 });
    expect(locateHunk("oh, say hello there", h)).toEqual({ from: 8, to: 13 });
    expect(locateHunk("say howdy there", h)).toBeNull();
    expect(locateHunk("hello hello", { ...h, from: 100, to: 105 })).toBeNull();
  });

  it("locates every hunk of a proposal in document order, or none", () => {
    const two: TextEdit = {
      path: "a",
      note: "",
      edits: [
        { from: 20, to: 25, before: "later", after: "LATER" },
        { from: 0, to: 5, before: "hello", after: "hi" },
      ],
    };
    expect(locateEdit("hello there, much later", two)).toEqual([
      { from: 1, to: 5, insert: "i" },
      { from: 18, to: 23, insert: "LATER" },
    ]);
    // One stale hunk makes the whole proposal stale.
    expect(locateEdit("hello there, much sooner", two)).toBeNull();
    // Two hunks that now overlap are not applied.
    const clash: TextEdit = {
      path: "a",
      note: "",
      edits: [
        { from: 0, to: 5, before: "hello", after: "hi" },
        { from: 3, to: 8, before: "lo th", after: "x" },
      ],
    };
    expect(locateEdit("hello there", clash)).toBeNull();
  });
});

describe("tool rows in words", () => {
  const row = (name: string, args: Record<string, unknown>, status = "done") => ({
    id: `${name}-${JSON.stringify(args)}`,
    name,
    risk: "read",
    status: status as "running" | "done" | "error",
    args,
    summary: "",
  });

  it("says each call as a sentence", () => {
    expect(rowLine(row("read_file", { path: "main.typ" }))).toBe("Read main.typ");
    expect(rowLine(row("read_file", { path: "main.typ" }, "running"))).toBe(
      "Reading main.typ",
    );
    expect(rowLine(row("search", { query: "Fourier" }))).toBe("Searched for “Fourier”");
    expect(rowLine(row("outline", {}))).toBe("Read the outlines");
    expect(rowLine(row("something_new", {}))).toBe("Ran something new");
  });

  it("folds consecutive calls into one row", () => {
    const rows = [
      row("read_file", { path: "a.typ" }),
      row("read_file", { path: "b.typ" }),
      row("read_file", { path: "c.typ" }),
      row("search", { query: "Fourier" }),
    ];
    expect(groupLine(rows)).toBe("Read 3 files, searched for “Fourier”");
    expect(groupLine([row("read_file", { path: "a.typ" })])).toBe("Read a.typ");
    expect(groupLine([row("bib_search", { query: "x" }), row("diagnostics", {})])).toContain(
      "checked the problems",
    );
  });

  it("shows only the running call, and keeps failures visible", () => {
    const rows = [
      row("read_file", { path: "a.typ" }),
      row("search", { query: "x" }, "running"),
    ];
    expect(groupLine(rows)).toBe("Searching for “x”…");
    expect(
      groupLine([row("read_file", { path: "a.typ" }), row("read_file", { path: "z" }, "error")]),
    ).toBe("Read 2 files, 1 failed");
  });
});

describe("transcript reducer", () => {
  const sid = "s1";
  const ev = (topic: string, extra: object = {}) => ({
    topic,
    payload: { session_id: sid, ...extra },
  });

  function run(events: { topic: string; payload: object }[]) {
    let state = begin(initialState(), "Rewrite", "a.md, selection (3 words)");
    for (const e of events) state = reduce(state, e.topic, e.payload);
    return state;
  }

  it("adopts the session of agent.started and streams text", () => {
    const state = run([
      ev("agent.started", { provider: "deepseek", model: "m", window: 100000 }),
      ev("agent.text", { text: "Hel" }),
      ev("agent.text", { text: "lo" }),
      ev("agent.done", { stop_reason: "end_turn" }),
    ]);
    expect(state.sessionId).toBe(sid);
    expect(state.running).toBe(false);
    expect(state.model).toBe("m");
    expect(state.usage.window).toBe(100000);
    expect(state.items.map((i) => i.kind)).toEqual(["user", "assistant"]);
    expect(state.items[1]).toMatchObject({ text: "Hello", open: false });
    expect(state.items[0]).toMatchObject({ context: "a.md, selection (3 words)" });
  });

  it("ignores another session's events", () => {
    let state = adopt(begin(initialState(), "x", ""), "mine");
    state = reduce(state, "agent.text", { session_id: "other", text: "no" });
    expect(state.items).toHaveLength(1);
    expect(reduce(state, "settings.changed", {})).toBe(state);
  });

  it("shows reasoning only as a signal, never as text", () => {
    let state = run([
      ev("agent.started"),
      ev("agent.text", { thinking: true, text: "secret plan" }),
    ]);
    expect(state.thinking).toBe(true);
    expect(state.items.map((i) => i.kind)).toEqual(["user"]);
    state = reduce(state, "agent.text", { session_id: sid, text: "Answer." });
    expect(state.thinking).toBe(false);
    expect(JSON.stringify(state)).not.toContain("secret plan");
  });

  it("folds a call and its result into one updating row in one group", () => {
    const state = run([
      ev("agent.started"),
      ev("agent.text", { text: "Reading." }),
      ev("agent.tool", { id: "t1", name: "read_file", risk: "read", status: "running", args: { path: "a.typ" }, summary: "" }),
      ev("agent.tool", { id: "t1", name: "read_file", risk: "read", status: "done", args: { path: "a.typ" }, summary: "a.typ lines 1-3 of 3" }),
      ev("agent.tool", { id: "t2", name: "search", risk: "read", status: "running", args: { query: "x" }, summary: "" }),
      ev("agent.text", { text: "Done." }),
    ]);
    expect(state.items.map((i) => i.kind)).toEqual(["user", "assistant", "tools", "assistant"]);
    const group = state.items[2];
    expect(group?.kind === "tools" && group.rows.map((r) => `${r.id}:${r.status}`)).toEqual([
      "t1:done",
      "t2:running",
    ]);
  });

  it("shows a queued edit as a proposal row, not as a tool row, and a refused edit as a failure", () => {
    const state = run([
      ev("agent.started"),
      ev("agent.tool", { id: "e1", name: "edit_text", risk: "propose", status: "running", args: {}, summary: "" }),
      ev("agent.tool", { id: "e1", name: "edit_text", risk: "propose", status: "error", args: {}, summary: "old text not found in a.md." }),
      ev("agent.tool", { id: "e2", name: "edit_text", risk: "propose", status: "running", args: {}, summary: "" }),
      ev("agent.tool", { id: "e2", name: "edit_text", risk: "propose", status: "done", args: {}, summary: "Proposal p1 queued" }),
      ev("agent.proposal", {
        proposal_id: "p1",
        kind: "text_edit",
        payload: { path: "a.md", note: "x", edits: [{ from: 0, to: 2, before: "ab", after: "c" }] },
      }),
    ]);
    const kinds = state.items.map((i) => i.kind);
    expect(kinds).toEqual(["user", "tools", "proposal"]);
    const group = state.items[1];
    expect(group?.kind === "tools" && group.rows.map((r) => r.id)).toEqual(["e1"]);
  });

  it("turns ask_user into a question the author can answer", () => {
    let state = run([
      ev("agent.started"),
      ev("agent.tool", { id: "a1", name: "ask_user", risk: "interaction", status: "running", args: {}, summary: "" }),
      ev("agent.tool", { id: "a1", name: "ask_user", risk: "interaction", status: "done", args: {}, summary: "", ask: { question: "Delete it?", choices: ["Yes", "No"] } }),
      ev("agent.done", { stop_reason: "ask_user" }),
    ]);
    expect(state.items.map((i) => i.kind)).toEqual(["user", "ask"]);
    expect(state.items[1]).toMatchObject({ question: "Delete it?", choices: ["Yes", "No"], answer: null });
    state = answerAsk(state, 1, "No");
    expect(state.items[1]).toMatchObject({ answer: "No" });
    expect(state.items.some((i) => i.kind === "notice")).toBe(false);
  });

  it("records a proposal, an approval, usage with a labelled estimate and an error", () => {
    let state = run([
      ev("agent.started"),
      ev("agent.approval", {
        call_id: "s1/t1",
        tool: "verify_citation",
        host: "api.crossref.org",
        scope: "network:api.crossref.org",
        after_untrusted_content: true,
      }),
      ev("agent.proposal", {
        proposal_id: "p1",
        kind: "text_edit",
        payload: { path: "a.md", note: "x", edits: [{ from: 0, to: 2, before: "ab", after: "c" }] },
      }),
      ev("agent.usage", {
        fill: 62000,
        window: 100000,
        cache_hit_percent: 80,
        total: { input: 10, output: 5, cache_read: 0, cache_write: 0, est_cost_micro_usd: 1234 },
      }),
      ev("agent.error", { code: "authentication_error", message: "Rejected", request_id: "req_1" }),
    ]);
    expect(pendingProposal(state)?.proposalId).toBe("p1");
    state = decideProposal(state, "p1", "accepted");
    expect(pendingProposal(state)).toBeUndefined();
    state = answerApproval(state, "s1/t1", false);
    expect(state.items.find((i) => i.kind === "approval")).toMatchObject({
      answer: "denied",
      host: "api.crossref.org",
      afterUntrusted: true,
    });
    expect(state.usage).toMatchObject({ input: 10, output: 5, costMicroUsd: 1234, fill: 62000, cacheHitPercent: 80 });
    expect(fillPercent(state.usage)).toBe(62);
    expect(formatCost(state.usage.costMicroUsd)).toBe("~$0.0012");
    expect(formatCost(null)).toBe("cost unknown");
    expect(state.items.at(-1)).toMatchObject({ kind: "error", requestId: "req_1" });
  });

  it("rejects a proposal payload that is not a list of located hunks", () => {
    const state = run([
      ev("agent.started"),
      ev("agent.proposal", { proposal_id: "p9", kind: "text_edit", payload: { path: "a", from: 0, to: 1, before: "a", after: "b" } }),
    ]);
    expect(state.items.at(-1)).toMatchObject({ kind: "proposal", edit: null });
  });

  it("says why a message stopped, in one quiet line", () => {
    const stopped = (reason: string, extra: object = {}) =>
      run([ev("agent.started"), ev("agent.done", { stop_reason: reason, ...extra })]).items.at(-1);
    expect(stopped("cancelled", { cancelled: true })).toMatchObject({ kind: "notice", text: "Stopped." });
    expect(stopped("iteration_cap", { steps: 12 })).toMatchObject({
      text: "Stopped after 12 steps. Say continue to go on.",
    });
    expect(stopped("context_full")).toMatchObject({ kind: "notice" });
    expect(stopped("end_turn")).toMatchObject({ kind: "user" });
  });
});
