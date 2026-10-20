import { describe, expect, it } from "vitest";

import {
  followStep,
  NOT_FOLLOWING,
  type FollowEvent,
  type FollowState,
} from "./liveFollow";

function run(events: readonly FollowEvent[], from: FollowState = NOT_FOLLOWING) {
  let state = from;
  const log = events.map((event) => {
    const step = followStep(state, event);
    state = step.state;
    return step;
  });
  return { state, log };
}

describe("follow mode", () => {
  it("opens the person's file and scrolls to their caret", () => {
    const { state, log } = run([
      {
        kind: "follow",
        peer: "7",
        viewing: "a.typ",
        at: { path: "b.typ", head: 40 },
      },
    ]);
    expect(log[0]?.effects).toEqual([
      { kind: "open", path: "b.typ" },
      { kind: "scroll", path: "b.typ", head: 40 },
    ]);
    expect(state).toMatchObject({ following: "7", expected: "b.typ" });
  });

  it("only scrolls when the person is in the file already shown", () => {
    const { log } = run([
      { kind: "follow", peer: "7", viewing: "a.typ", at: { path: "a.typ", head: 3 } },
      { kind: "peer-at", peer: "7", path: "a.typ", head: 90 },
    ]);
    expect(log[0]?.effects).toEqual([{ kind: "scroll", path: "a.typ", head: 3 }]);
    expect(log[1]?.effects).toEqual([{ kind: "scroll", path: "a.typ", head: 90 }]);
  });

  it("follows the person into another file, and the arrival it caused does not end it", () => {
    const { state, log } = run([
      { kind: "follow", peer: "7", viewing: "a.typ", at: null },
      { kind: "peer-at", peer: "7", path: "b.typ", head: null },
      { kind: "navigated", path: "b.typ" },
      { kind: "peer-at", peer: "7", path: "c.typ", head: 5 },
      { kind: "navigated", path: "c.typ" },
    ]);
    expect(log[1]?.effects).toEqual([{ kind: "open", path: "b.typ" }]);
    expect(log[2]?.ended).toBeNull();
    expect(log[3]?.effects).toEqual([
      { kind: "open", path: "c.typ" },
      { kind: "scroll", path: "c.typ", head: 5 },
    ]);
    expect(state).toEqual({ following: "7", viewing: "c.typ", expected: null });
  });

  it("does not ask to open the same file twice while it is opening", () => {
    const { log } = run([
      { kind: "follow", peer: "7", viewing: "a.typ", at: { path: "b.typ", head: 1 } },
      { kind: "peer-at", peer: "7", path: "b.typ", head: 2 },
    ]);
    expect(log[1]?.effects).toEqual([{ kind: "scroll", path: "b.typ", head: 2 }]);
  });

  it("ends on navigation of your own", () => {
    const { state, log } = run([
      { kind: "follow", peer: "7", viewing: "a.typ", at: null },
      { kind: "navigated", path: "z.typ" },
    ]);
    expect(log[1]?.ended).toBe("navigation");
    expect(state.following).toBeNull();
    expect(state.viewing).toBe("z.typ");
  });

  it("ends on an edit", () => {
    const { state, log } = run([
      { kind: "follow", peer: "7", viewing: "a.typ", at: null },
      { kind: "edited" },
    ]);
    expect(log[1]?.ended).toBe("edit");
    expect(state.following).toBeNull();
  });

  it("ends when the person leaves, and ignores others leaving", () => {
    const { log } = run([
      { kind: "follow", peer: "7", viewing: null, at: null },
      { kind: "peer-gone", peer: "8" },
      { kind: "peer-gone", peer: "7" },
    ]);
    expect(log[1]?.ended).toBeNull();
    expect(log[2]?.ended).toBe("left");
  });

  it("stops on request; later movement does nothing", () => {
    const { state, log } = run([
      { kind: "follow", peer: "7", viewing: "a.typ", at: null },
      { kind: "unfollow" },
      { kind: "peer-at", peer: "7", path: "b.typ", head: 1 },
      { kind: "edited" },
    ]);
    expect(log[1]?.ended).toBe("stopped");
    expect(log[2]?.effects).toEqual([]);
    expect(log[3]?.ended).toBeNull();
    expect(state.following).toBeNull();
  });

  it("ignores other people's movement while following one", () => {
    const { log } = run([
      { kind: "follow", peer: "7", viewing: "a.typ", at: null },
      { kind: "peer-at", peer: "8", path: "b.typ", head: 1 },
    ]);
    expect(log[1]?.effects).toEqual([]);
  });

  it("tracks the file in front while not following", () => {
    const { state } = run([{ kind: "navigated", path: "q.typ" }]);
    expect(state.viewing).toBe("q.typ");
  });
});
