# Using the AI agent (private beta)

The agent helps you rewrite, shorten, translate and explain text, fix compile problems, and
chat about your document. It uses **your own key** with a provider you choose. Nothing is sent
until you press Send (or pick an action), and every change it suggests is a proposal you accept
or reject.

## Open the Agent

The Agent is a chat column on the right of the window, beside the preview. Open it with the
**Agent** button at the bottom of the activity bar, with **Ctrl+Shift+A** (Cmd+Shift+A on a
Mac), from **View, Agent**, or by clicking **AI: ready / not set up / off** in the status bar.
Press the same button or shortcut again to close it. Drag its left edge to resize it (it
remembers the width); in a narrow window (under 900 px) it floats over the editor instead.

The header shows the model in use (open it to pick another model or provider), **New chat**,
and **History** with earlier chats of this project. Your messages are blocks labelled
**You**; the agent's answers flow as text with lists and code blocks (each with a Copy
button). Tool use shows as one quiet line such as "Read main.typ" that expands for details,
and "Thinking" expands to the model's reasoning when the provider sends it.

## Set up with DeepSeek

The first time, the Agent shows a three-step card:

1. **What it does.** A short description.
2. **Choose a provider and add your key.** Pick `DeepSeek`, paste the key and press
   **Save key and test**. Create the key at <https://platform.deepseek.com> (API keys) and add
   a little balance first. The key goes to your system keychain and is never shown again. A
   friendly message tells you if the key, balance or address is wrong. "Not saved: stored for
   this session only" in Settings means no keychain was available and you must paste it again
   next time.
3. **Agree to share what you send.** The card states what is sent and where. Press **Agree
   and start**.

The same choices are in **Settings, Agent** (Ctrl+, then Agent), in groups: **Provider**,
**Model**, **Key**, **Privacy** and **Cost**. For DeepSeek choose the model `deepseek-flash`
(fast, cheap) or `deepseek-v4-pro` (stronger); these are the names on api-docs.deepseek.com
as of 2026-10-18 (the old `deepseek-v4-flash` still works but is retired). Leave **Base URL**
empty.

If the agent is **off** (Settings, Provider, Enable the AI agent), the card says so and offers
**Turn on**. In **Offline mode** only a model on this computer (for example Ollama) is
allowed, and the card says how to change that.

## Ask and edit

* Type a question in the box at the bottom. **Enter** sends, **Shift+Enter** adds a line.
  **Send** becomes **Stop** while the agent works. Nothing is sent until you press Send.
* Before the first message the chat offers four starting points: **Rewrite selection**,
  **Explain this error**, **Summarise this section** and **Draft a figure caption**.
* Select text and use **Edit, AI, Rewrite selection** (or Ctrl+K and type "AI").
* The result appears in the editor as an inline diff (removed text struck through, new text
  in green) and in the chat as a card with the file name and how many lines change.
  **Accept** (the one lime button) applies it as one edit; **Reject** or Esc keeps your text.
  After accepting, **Undo** on the card (or Ctrl+Z) takes it back. When several edits wait,
  **Apply all** accepts them one by one.
* Some tools ask first (for example a network call). The chat shows a card with **Allow
  once**, **Allow for this chat** and **Deny**.
* If a request fails, the message says what to do and offers **Retry**.

## What is sent

Only your message plus the chips above the input (default: the selection). Each chip shows
its size and has a remove button; the **+** button or typing **@** attaches more (the current
file, problems, outline, bibliography keys, a figure, another open file, or a pinned note).
**Preview what will be sent**, under the box, shows the exact request before anything leaves
your machine, including secret-like strings that were redacted. The
document is sent as delimited data, never as instructions. DeepSeek's reasoning text is kept
in the session record and shown dimmed only if you enable "Show the model's reasoning".

Costs shown are estimates from published prices; the provider's bill is the truth.

## Fix with AI

In the Problems list (bottom panel), an error with a file shows **Fix with AI**. It opens the
Agent if it is closed, sends the diagnostic and the lines around it, and proposes an edit as an
inline diff. After you accept, the normal
compile runs; Manang does not yet check automatically that the fix worked.

## Where things are stored

`.manang/agent/sessions/<id>/` in your project (`transcript.jsonl`, `audit.jsonl`,
`meta.toml`), excluded from git by `.manang/agent/.gitignore`. Old sessions are deleted after
the retention setting (30 days). No key is ever written there.

## Turning it off

* Settings, Agent, Provider, **Enable the AI agent** off: every agent command refuses.
* **Offline mode**: only a server on this machine (for example Ollama) is allowed.
* A project can switch it off for itself with `.manang/agent/policy.toml`:
  `enabled = false` (a project can only tighten, never loosen).
* Remove the key under **Key**, and withdraw consent under **Privacy**, in Settings, Agent.

## Browser build

`pnpm dev` uses a clearly labelled **mock provider** that streams canned text and a placeholder
edit, so the interface can be tried without a key. The desktop app talks to the real provider.
