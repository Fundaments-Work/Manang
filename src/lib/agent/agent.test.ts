import { describe, expect, it } from "vitest";

import {
  chipTokens,
  clearUnpinned,
  estimateTokens,
  mergeTray,
  removeChip,
  selectionChip,
  fileChip,
  togglePin,
  toWire,
  tokenLabel,
  totalTokens,
} from "./chips";
import { describeEdit, diffSegments, locateEdit, minimalChange } from "./proposal";
import {
  adopt,
  answerApproval,
  begin,
  decideProposal,
  formatCost,
  initialState,
  pendingProposal,
  reduce,
} from "./reducer";

describe("chip token estimates", () => {
  it("uses about 3.5 characters per token, rounded up, like the host", () => {
    expect(estimateTokens("")).toBe(0);
    expect(estimateTokens("abcdefg")).toBe(2);
    expect(estimateTokens("a".repeat(35))).toBe(10);
    expect(estimateTokens("😀😀😀😀😀😀😀")).toBe(2);
  });

  it("caps a chip like the host does", () => {
    const big = fileChip("a.typ", "x".repeat(200_000));
    expect(big.tokens).toBe(estimateTokens("x".repeat(80_000)));
  });

  it("builds tray chips with ranges, labels and totals", () => {
    const s = selectionChip("main.typ", 3, 12, "hello world");
    expect(s.label).toBe("Selection in main.typ");
    expect(s.range).toEqual([3, 12]);
    expect(totalTokens([s, fileChip("main.typ", "abcdefg")])).toBe(s.tokens + 2);
    expect(chipTokens({ kind: "pinned", text: "abcdefg" })).toBe(2);
    expect(tokenLabel(950)).toBe("950");
    expect(tokenLabel(1234)).toBe("1.2k");
  });

  it("does not send tray-only fields", () => {
    const wire = toWire(selectionChip("a.md", 0, 1, "x"));
    expect("tokens" in wire).toBe(false);
    expect(wire.text).toBe("x");
  });

  it("replaces a selection by a new one, keeps pins, removes and clears", () => {
    const a = selectionChip("a.md", 0, 1, "x");
    const f = fileChip("a.md", "text");
    const b = selectionChip("a.md", 5, 9, "yyyy");
    const merged = mergeTray([a, f], [b]);
    expect(merged.map((c) => c.id)).toEqual([f.id, b.id]);
    const pinned = togglePin(merged, f.id);
    expect(clearUnpinned(pinned).map((c) => c.id)).toEqual([f.id]);
    expect(removeChip(pinned, f.id).map((c) => c.id)).toEqual([b.id]);
    expect(togglePin(pinned, f.id).find((c) => c.id === f.id)?.pinned).toBe(false);
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

  it("finds the change in editor coordinates", () => {
    const edit = {
      path: "a.typ",
      from: 10,
      to: 23,
      before: "the quick fox",
      after: "the slow fox",
      explanation: "",
    };
    expect(minimalChange(edit, { from: 10, to: 23 })).toEqual({
      from: 14,
      to: 19,
      insert: "slow",
    });
    expect(describeEdit(edit)).toContain("Replace 13 characters with 12");
  });

  it("locates the edit, follows moved text and calls changed text stale", () => {
    const edit = { path: "a", from: 4, to: 9, before: "hello", after: "hi", explanation: "" };
    expect(locateEdit("say hello there", edit)).toEqual({ from: 4, to: 9 });
    expect(locateEdit("oh, say hello there", edit)).toEqual({ from: 8, to: 13 });
    expect(locateEdit("say howdy there", edit)).toBeNull();
    expect(locateEdit("hello hello", { ...edit, from: 100, to: 105 })).toBeNull();
  });
});

describe("transcript reducer", () => {
  const sid = "s1";
  const ev = (topic: string, extra: object = {}) => ({
    topic,
    payload: { session_id: sid, ...extra },
  });

  function run(events: { topic: string; payload: object }[]) {
    let state = begin(initialState(), "Rewrite", ["Selection in a.md"]);
    for (const e of events) state = reduce(state, e.topic, e.payload);
    return state;
  }

  it("adopts the session of agent.started and streams text", () => {
    const state = run([
      ev("agent.started", { provider: "deepseek", model: "m" }),
      ev("agent.text-delta", { text: "Hel" }),
      ev("agent.text-delta", { text: "lo" }),
      ev("agent.done", { stop_reason: "end_turn" }),
    ]);
    expect(state.sessionId).toBe(sid);
    expect(state.running).toBe(false);
    expect(state.model).toBe("m");
    expect(state.items.map((i) => i.kind)).toEqual(["user", "assistant"]);
    expect(state.items[1]).toMatchObject({ text: "Hello", open: false });
  });

  it("ignores another session's events", () => {
    let state = adopt(begin(initialState(), "x", []), "mine");
    state = reduce(state, "agent.text-delta", { session_id: "other", text: "no" });
    expect(state.items).toHaveLength(1);
    expect(reduce(state, "settings.changed", {})).toBe(state);
  });

  it("keeps reasoning apart from the answer and tool calls on their own lines", () => {
    const state = run([
      ev("agent.started"),
      ev("agent.thinking-delta", { text: "plan" }),
      ev("agent.text-delta", { text: "Reading." }),
      ev("agent.tool-call", { id: "t1", name: "doc_read", risk: "read", args: {} }),
      ev("agent.tool-result", { id: "t1", is_error: false, summary: "ok" }),
      ev("agent.text-delta", { text: "Done." }),
    ]);
    expect(state.items.map((i) => i.kind)).toEqual([
      "user",
      "assistant",
      "tool",
      "assistant",
    ]);
    expect(state.items[1]).toMatchObject({ thinking: "plan", text: "Reading." });
    expect(state.items[2]).toMatchObject({ state: "done", summary: "ok" });
  });

  it("records a proposal, an approval, usage with a labelled estimate and an error", () => {
    let state = run([
      ev("agent.started"),
      ev("agent.approval-requested", {
        call_id: "s1/t1",
        tool: "net_fetch",
        risk: "network",
        scope: "network",
        after_untrusted_content: true,
      }),
      ev("agent.proposal", {
        proposal_id: "p1",
        kind: "text_edit",
        payload: { path: "a.md", from: 0, to: 2, before: "ab", after: "c", explanation: "x" },
      }),
      ev("agent.usage", {
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
      afterUntrusted: true,
    });
    expect(state.usage).toMatchObject({ input: 10, output: 5, costMicroUsd: 1234 });
    expect(formatCost(state.usage.costMicroUsd)).toBe("~$0.0012");
    expect(formatCost(null)).toBe("cost unknown");
    expect(state.items.at(-1)).toMatchObject({ kind: "error", requestId: "req_1" });
  });

  it("shows a cancellation as a quiet notice", () => {
    const state = run([ev("agent.started"), ev("agent.cancelled"), ev("agent.done", { stop_reason: "cancelled" })]);
    expect(state.items.at(-1)).toMatchObject({ kind: "notice" });
    expect(state.stopReason).toBe("cancelled");
  });
});
