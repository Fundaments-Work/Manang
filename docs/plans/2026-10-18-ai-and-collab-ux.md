# Making the AI agent and live editing discoverable and professional

> Status (2026-10-20): built (Agent view, Share button and popover, presence, status chips). The live-editing parts are extended by `2026-10-20-live-collab-v2.md`.

## Problem

Both features work but are hidden. The agent is a tab in the bottom panel (closed by default); live editing exists only in File > Share this file live and the palette. There is no activity-bar entry, no status indicator, no Share button, and Git sharing (Versions view) and live sessions are two unrelated places. A new tester cannot find either.

## Decisions (change them in review, not in code)

1. **Agent is a first-class view.** An activity-bar entry "Agent" (icon, aria-label, tooltip with shortcut) opens it in the side panel like Files and Figures, resizable, with the existing transcript, context tray, preview and diff flow. The bottom panel keeps Problems and Output; "Fix with AI" and Edit > AI open the Agent view. UI vocabulary gains one word: **Agent**.
2. **One Share entry.** A **Share** button at the end of the tab strip (one lime action only when a session or update is pending, otherwise ordinary) opens a popover with three plain rows: **Edit together live** (start or join), **Share versions** (the Git connect, Share, Get updates from Versions), **Invite link** (copy, while a session is live). Everything also stays in File and the palette.
3. **Presence is visible.** While a session is live the tab strip shows participants (initials in the collaborator tints), the LiveBar under the tabs stays, and the status bar shows `Live: 2 people` which opens the popover. Not in a session: nothing is shown.
4. **Setup is guided, never a dead end.** First use of either feature shows a three-step card (what it does, what it needs, one button): Agent: pick a provider, paste a key, Test connection, consent. Live: set the relay address (or "ask your lab for one"), your display name. Settings gets two clear pages, **Agent** and **Collaboration**, with the same fields.
5. **Status chips** in the status bar: `AI: ready | not set up | off`, `Live: ...`. Quiet when off, one click to the right place.
6. **Professional pass:** consistent wording (no jargon: not push/pull/CRDT/relay in primary text; "relay" appears only in Settings with an explanation), empty states, errors that say what to do, keyboard access (Ctrl+Shift+A for the Agent, Ctrl+Alt+L for live, shown in menus), reduced-motion and contrast per the design system, light theme checked, narrow widths checked.

## Work packages

| WP | Who | Scope | Done when |
| :--- | :--- | :--- | :--- |
| 1 Agent view | agent | activity-bar entry, side view with resize, move from bottom tab, Fix with AI and Edit > AI open it, setup card, status chip, Settings Agent page tidy, shortcut, docs and screenshots | verify-ui section; every flow reachable without the palette |
| 2 Share and presence | agent | Share button and popover (live + versions + invite), tab-strip participants, status chip, setup card for live, Settings Collaboration page, shortcut, docs and screenshots | verify-ui section; start, invite, join, leave and Share versions reachable from the popover |
| 3 Review and polish | lead | wording audit, light theme and narrow checks, screenshot refresh for the README, a11y pass with Antigravity | owner walkthrough clean |

WP1 and WP2 both touch `ActivityBar`, `StatusBar`, `TabStrip` and `layout.svelte.ts`: WP1 owns the activity bar and layout; WP2 owns the tab strip and the Share popover; both add one status chip and merge with the union.

## Not in this pass

Relay hosting, policy keys (`collab.live`, `collab.relays`), live figures, comments. The relay field says what it needs until a host exists.
