# Using the AI agent (private beta)

The agent is a writing assistant for your Typst, LaTeX and Markdown documents. It helps you
write, revise, translate, explain and debug your own text. It uses **your own key** with a
provider you choose. Nothing is sent until you press Send (or pick an action), and every change
it suggests is a proposal you accept or reject. It cannot change a file by itself.

## Open the Agent

The Agent is a chat column on the right of the window, beside the preview. Open it with the
**Agent** button at the bottom of the activity bar, with **Ctrl+Shift+A** (Cmd+Shift+A on a
Mac), from **View, Agent**, or by clicking **AI: ready / not set up / off** in the status bar.
Press the same button or shortcut again to close it. Drag its left edge to resize it (it
remembers the width); in a narrow window (under 900 px) it floats over the editor instead.

The header is thin: the model in use (open it to pick another model or provider), **New chat**,
and a **more** menu with earlier chats, **Show what will be sent**, and the session's cost
estimate and cache hits. Your messages are blocks labelled **You**; the assistant's answers flow
as text with lists and code blocks. While it reads your project, the chat shows **one quiet
row** such as "Read main.typ, searched for “Fourier”" (click it to see each step). Its
reasoning is never shown as text: you only see "Thinking…" while it works.

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
**Model**, **Key**, **Privacy**, **Cost** and the working window. For DeepSeek choose the model
`deepseek-flash` (fast, cheap) or `deepseek-v4-pro` (stronger). Leave **Base URL** empty.

**Reasoning effort** (Settings, Agent, Model) controls how long reasoning models think. The
default, `auto`, is `low` for slash commands such as `/rewrite` (quick edits) and `high` for open
chat. It is fixed when a chat starts, because changing it mid-chat would make the provider
re-read the whole conversation at full price. Whether `low` is good enough for your texts is
something to measure with the live eval below; Manang does not assume it.

If the agent is **off** (Settings, Provider, Enable the AI agent), the card says so and offers
**Turn on**. In **Offline mode** only a model on this computer (for example Ollama) is
allowed, and the card says how to change that.

## Ask and edit

* Type in the box at the bottom. **Enter** sends, **Shift+Enter** adds a line. **Send**
  becomes **Stop** while the assistant works.
* **Slash commands.** Type **/** to list them: `/rewrite`, `/shorten`, `/translate <language>`,
  `/explain`, `/fix` and `/cite`. Select text first; the command works on the selection. The
  Edit, AI menu items and **Fix with AI** in Problems send the same commands, so there is one
  way to do everything. `/fix` checks its own proposal before you accept it (for Typst
  projects it compiles the project as if your edit were applied).
* **Your own commands.** A Markdown file in `.manang/commands/` (for the project) or in the
  `commands` folder of your Manang config (for you) becomes a command named after the file.
  An optional `description:` line in front matter is shown in the menu, and `$ARGUMENTS` is
  replaced by what you type after the name. A project command overrides a built-in of the same
  name.
* **Project instructions.** Put a `MANANG.md` in the project root (plain Markdown: spelling
  variant, citation style, notation, files not to touch). The assistant reads it once per chat.
  Keep it under about 2,000 tokens; longer text is cut with a notice. A `MANANG.md` in your
  Manang config folder adds personal rules. `AGENTS.md` or `CLAUDE.md` are read only when you
  turn on **Use AGENTS.md as project instructions** (off by default: in a research repository
  they are usually build notes for coding agents) and there is no `MANANG.md`.
* **Proposals.** The result appears in the editor as an inline diff (removed text struck
  through, new text in green) and in the chat as **one row**: "Edit to main.typ, 2 changes"
  with **Review** (shows the diff), **Accept** (the one lime button) and **Reject**. **Accept**
  applies all changes as one edit; **Reject** or Esc keeps your text. After accepting,
  **Undo** on the row (or Ctrl+Z) takes it back. When several edits wait, **Apply all**
  accepts them one by one.
* **Questions.** Before anything destructive or wide (deleting a section, touching many files,
  renaming labels) the assistant asks one short question, with buttons, and waits.
* **Permission.** A tool that reaches the network, such as checking a citation against
  Crossref, asks first in one line: "Allow verify citation (api.crossref.org)?" with **Allow**,
  **Allow for this chat** and **Deny**. After the chat has read content from outside your
  project, it asks again every time.
* If a request fails, the message says what to do and offers **Retry**, which runs the
  conversation again without repeating your message.

## What the assistant sees

The model gets a short, constant instruction text, the same for every request (so the provider
can cache it). It learns about your project from a small overview in the **first message of a
chat**: the project name, the formats in it, the file tree and your `MANANG.md`. With each message it also gets
the file you are in, your cursor line, your **selection**, and anything you mention with **@**
(another open file, the **problems**, or the figure). Everything else it reads **on demand**
with plain tools, each result capped at about 6,000 tokens: list files, read a file (the
editor's text, including unsaved changes), search, outline, problems, and bibliography
search. It reads your open, unsaved text, never an older copy.

The line under the box shows what travels with your next message: "main.typ, selection (38
words)". Mentions appear there with a small **x** to remove them. **Show what will be sent**
(header menu) shows the exact request, including secret-like strings that were redacted,
before anything leaves your machine. Documents are sent as delimited data, never as
instructions. Costs shown are estimates from published prices; the provider's bill is the
truth.

### Long chats

The chat keeps a **working window** (Settings, Agent, Cost; default 100,000 tokens, even for
models that could take more: cost and quality fall first). Past 50% a small "% of the window"
appears under the box. At 60% the old tool results (file contents it already read) are replaced
by one-line stubs; your messages and its edits are never cleared. At 85% the early part of the
chat is summarised into a short note and the chat continues with the note and your last two
messages. The full chat stays on disk; only what is sent changes. If a chat is still too long
after that, it stops and asks you to start a new one.

## Fix with AI

In the Problems list (bottom panel), an error with a file shows **Fix with AI**. It opens the
Agent if it is closed and runs `/fix` with that diagnostic and the lines around it. For Typst
projects the assistant checks its proposed fix by compiling a scratch copy (the preview is not
touched) before you accept it, and tries at most twice more if problems remain.

## Where things are stored

`.manang/agent/sessions/<id>/` in your project (`transcript.jsonl`, `audit.jsonl`,
`meta.toml`), excluded from git by `.manang/agent/.gitignore`. Old sessions are deleted after
the retention setting (30 days). No key is ever written there.

## Turning it off

* Settings, Agent, Provider, **Enable the AI agent** off: every agent command refuses.
* **Offline mode**: only a server on this machine (for example Ollama) is allowed.
* A project can switch it off for itself with `.manang/agent/policy.toml`:
  `enabled = false` (a project can only tighten, never loosen), or lower the working window
  with `max_context_tokens`.
* Remove the key under **Key**, and withdraw consent under **Privacy**, in Settings, Agent.

## Browser build

`pnpm dev` uses a clearly labelled **mock provider** that streams canned text, a few tool rows
and a placeholder edit, so the interface can be tried without a key. The desktop app talks to
the real provider.

## For developers: the eval suite

`cargo test -p manang-agent --test golden` runs nineteen golden tasks (rewrite, shorten,
translate, explain, grammar-only, citations that exist and do not, an injection inside a
selection, one spelling across five files, LaTeX and Typst fixes, steering retries, escaping,
a 60-call session that crosses both compaction thresholds, cancellation) against a scripted
provider. It runs in CI and needs no key; each task has a code grader, and the graders are
checked against bad runs too.

To run the same tasks against a real model, three trials each, with pass^3, tool calls, tokens,
cache-hit ratio and cost:

```
MANANG_EVAL_LIVE=1 MANANG_EVAL_PROVIDER=deepseek MANANG_EVAL_MODEL=deepseek-flash \
  cargo test -p manang-agent --test golden live_ -- --nocapture
```

It uses the key in your keystore (never printed) and counts as your consent to send the
fixtures to that provider. It never runs in CI and never gates a build; use it after changing
the system prompt, a tool, or the reasoning effort.
