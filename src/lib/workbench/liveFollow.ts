/**
 * Follow mode: this window shows what one other person is doing. A pure state
 * machine, so the rules are tested without an editor.
 *
 *  - Following a person opens the file they have in front of them and scrolls to
 *    their caret; it keeps doing so as they move.
 *  - Any navigation of your own (another tab, another file) or any edit ends it.
 *    Navigation the follow itself caused is recognised (`expected`) and does not.
 *  - The person leaving the session ends it too.
 */

export interface FollowState {
  /** Peer id being followed. */
  readonly following: string | null;
  /** The file in front of this window, as last reported. */
  readonly viewing: string | null;
  /** A file the follow itself asked to open and has not seen arrive yet. */
  readonly expected: string | null;
}

export const NOT_FOLLOWING: FollowState = {
  following: null,
  viewing: null,
  expected: null,
};

export type FollowEvent =
  | {
      kind: "follow";
      peer: string;
      viewing: string | null;
      /** Where the person is now, if known. */
      at: { path: string; head: number | null } | null;
    }
  | { kind: "unfollow" }
  | { kind: "peer-at"; peer: string; path: string | null; head: number | null }
  | { kind: "navigated"; path: string | null }
  | { kind: "edited" }
  | { kind: "peer-gone"; peer: string };

export type FollowEffect =
  { kind: "open"; path: string } | { kind: "scroll"; path: string; head: number };

export type FollowEnd = "stopped" | "navigation" | "edit" | "left";

export interface FollowStep {
  readonly state: FollowState;
  readonly effects: readonly FollowEffect[];
  /** Why follow ended in this step, if it did. */
  readonly ended: FollowEnd | null;
}

function end(state: FollowState, why: FollowEnd): FollowStep {
  if (state.following === null) return { state, effects: [], ended: null };
  return {
    state: { ...state, following: null, expected: null },
    effects: [],
    ended: why,
  };
}

/** Where the followed person is: open their file if it is not shown, then scroll. */
function toward(
  state: FollowState,
  path: string | null,
  head: number | null,
): FollowStep {
  if (path === null) return { state, effects: [], ended: null };
  const effects: FollowEffect[] = [];
  let next = state;
  if (path !== state.viewing && path !== state.expected) {
    effects.push({ kind: "open", path });
    next = { ...state, expected: path };
  }
  if (head !== null) effects.push({ kind: "scroll", path, head });
  return { state: next, effects, ended: null };
}

export function followStep(state: FollowState, event: FollowEvent): FollowStep {
  switch (event.kind) {
    case "follow": {
      const base: FollowState = {
        following: event.peer,
        viewing: event.viewing,
        expected: null,
      };
      return toward(base, event.at?.path ?? null, event.at?.head ?? null);
    }
    case "unfollow":
      return end(state, "stopped");
    case "peer-at":
      if (state.following !== event.peer) return { state, effects: [], ended: null };
      return toward(state, event.path, event.head);
    case "navigated": {
      if (state.following === null) {
        return {
          state: { ...state, viewing: event.path },
          effects: [],
          ended: null,
        };
      }
      if (event.path !== null && event.path === state.expected) {
        return {
          state: { ...state, viewing: event.path, expected: null },
          effects: [],
          ended: null,
        };
      }
      if (event.path === state.viewing) return { state, effects: [], ended: null };
      const ended = end(state, "navigation");
      return { ...ended, state: { ...ended.state, viewing: event.path } };
    }
    case "edited":
      return end(state, "edit");
    case "peer-gone":
      return state.following === event.peer
        ? end(state, "left")
        : { state, effects: [], ended: null };
  }
}
