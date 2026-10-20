# AI Assistant

Use the AI assistant to rewrite, shorten, translate, explain and fix your text with your own key; it only proposes changes and never edits a file by itself.

The assistant helps you write, revise, translate, explain and debug your own Typst, LaTeX and Markdown text. It uses **your own key** with a provider you choose. Nothing is sent until you press **Send** or pick an action, and every change it suggests is a proposal that you accept or reject. It cannot change a file by itself, and one switch turns it off completely.

## 1. Open the Agent

The assistant lives in the **Agent** column on the right of the window, beside the preview.

1. Open it in any of these ways:
   - Click the **Agent** button near the bottom of the Activity bar.
   - Press `Ctrl+Shift+A` (`Cmd+Shift+A`).
   - Choose **View > Agent**.
   - Click **AI: ready**, **AI: not set up** or **AI: off** in the status bar.
2. Press the same button or shortcut again, or click the close button in the header, to close it.
3. Drag the left edge of the column to resize it. Manang remembers the width. In a window narrower than 900 pixels the column floats over the editor instead.

The header shows the model in use (open it to pick another model or provider), **New chat**, and a **more** menu with earlier chats, **Show what will be sent** and the session's cost estimate. Your messages are labelled **You** and the answers are labelled **Assistant**. While the assistant reads your project, the chat shows one quiet row such as "Read main.typ, searched for Fourier"; click it to see each step. The model's reasoning is never shown as text, only "Thinking" while it works.

## 2. Set up a provider

The first time you open the Agent, a three-step card appears.

1. Read **What it does**.
2. Under **Choose a provider and add your key**, pick a provider, paste your key and press **Save key and test**. The key goes to your system keychain and is never shown again. If the key, balance or address is wrong, a message says which. Providers that run on your computer, such as Ollama, need no key.
3. Under **Agree to share what you send**, read what is sent and where, then press **Agree and start**.

The available providers are Anthropic (the default), OpenAI, DeepSeek, OpenRouter, Groq, Ollama, llama.cpp, LM Studio and a custom OpenAI-compatible address. Create the key on your provider's website; some providers need a little balance on the account first.

The same choices are in **Settings > Plugins > Agent**, in groups: **Provider**, **Model**, **Key**, **Privacy** and **Cost**. Leave **Base URL** empty unless you use a custom or local server. If the settings say "Not saved: stored for this session only", no system keychain was available and you must paste the key again next time.

**Reasoning effort** controls how long reasoning models think. The default, `auto`, uses `low` for slash commands such as `/rewrite` and `high` for open chat. It is fixed when a chat starts.

If the agent is **off** (**Settings > Plugins > Agent > Enable the AI agent**), the card says so and offers **Turn on**. In **Offline mode** only a model on this computer is allowed, and the card tells you how to change that.

## 3. Ask and edit

1. Type in the box at the bottom. `Enter` sends and `Shift+Enter` adds a line. **Send** becomes **Stop** while the assistant works.
2. To work on part of your text, select it first. The line under the box shows what travels with your next message, for example "main.typ, selection (38 words)".
3. Review what comes back as a proposal (see below).

### Slash commands

Type `/` to list the commands:

| Command | What it does |
| --- | --- |
| `/rewrite` | Rewrite the selection for clarity |
| `/shorten` | Make the selection shorter |
| `/translate <language>` | Translate the selection |
| `/explain` | Explain the selection |
| `/fix` | Fix the problem under the cursor |
| `/cite` | Find citations for the selection |

The first four also appear in the **Edit** menu as **Rewrite selection**, **Shorten selection**, **Translate selection** and **Explain selection**, so there is one way to do everything. `/fix` checks its own proposal before you accept it; for Typst projects it compiles a scratch copy of the project as if your edit were applied, and the preview is not touched.

You can add your own commands. A Markdown file in `.manang/commands/` inside the project, or in the `commands` folder of Manang's configuration folder, becomes a command named after the file. An optional `description:` line in front matter is shown in the list, and `$ARGUMENTS` is replaced by what you type after the name. A project command overrides a built-in command of the same name.

### Project instructions

Put a `MANANG.md` file in the project root to tell the assistant your conventions: spelling variant, citation style, notation, files not to touch. The assistant reads it once per chat. Keep it under about 2,000 tokens; longer text is cut and a notice says so. A `MANANG.md` in Manang's configuration folder adds personal rules. `AGENTS.md` and `CLAUDE.md` are read only when you turn on **Use AGENTS.md as project instructions** (off by default) and the project has no `MANANG.md`.

### Proposals

A proposal appears in the editor as an inline diff, with removed text struck through and new text in green. In the chat it is one row, for example "Edit to main.typ, 2 changes", with three buttons:

- **Review** shows the diff in the editor.
- **Accept** applies all the changes as one edit.
- **Reject**, or `Esc` in the editor, keeps your text as it is.

After you accept, **Undo** on the row (or `Ctrl+Z`) takes the edit back. When several proposals wait, **Apply all** accepts them one by one.

### Questions and permission

- Before anything destructive or wide, such as deleting a section, touching many files or renaming labels, the assistant asks one short question with buttons and waits.
- A tool that uses the network, such as checking a citation against Crossref, asks first: **Allow**, **Allow for this chat** or **Deny**. After a chat has read content from outside your project, it asks every time.
- If a request fails, the message says what to do and offers **Retry**, which runs the conversation again without repeating your message.

## 4. Fix with AI

In the **Problems** panel, an error that belongs to a file shows **Fix with AI**. It opens the Agent if it is closed and runs `/fix` with that problem and the lines around it. Manang's own one-click **Fix** button is separate and never uses an AI model; see [Fixing Errors](fixing-errors.md).

## 5. What the assistant sees

- **First message of a chat:** a small overview of the project: its name, the formats in it, the file tree and your `MANANG.md`.
- **With each message:** the file you are in, your cursor line, your selection, and anything you mention with `@` (another open file, the problems, or the open figure). Mentions appear under the box with a small **x** to remove them.
- **On demand:** the assistant reads the rest with plain tools, each result capped at about 6,000 tokens: list files, read a file, search, outline, problems and bibliography search. It reads your open text, including unsaved changes.

Choose **Show what will be sent** in the header menu to see the exact request before anything leaves your computer, including secret-like strings that were redacted. Documents are sent as data, never as instructions. In **Settings > Plugins > Agent > Privacy** you can also redact email addresses and user names in file paths. Costs shown are estimates from published prices; your provider's bill is the truth.

### Long chats

The chat keeps a working window of 100,000 tokens by default (**Settings > Plugins > Agent > Cost**). Past 50% a small percentage appears under the box. At 60% old tool results are replaced by one-line stubs; your messages and its edits are never cleared. At 85% the early part of the chat is summarised into a short note and the chat continues. The full chat stays on disk. If a chat is still too long after that, it stops and asks you to start a new one.

## 6. Where chats are stored

Each chat is stored in `.manang/agent/sessions/<id>/` inside the project, and that folder is excluded from Git. Sessions older than the **Keep sessions (days)** setting, 30 by default, are deleted when a new one starts. No key is ever written there.

## 7. Turn it off

- **Settings > Plugins > Agent > Enable the AI agent**: turn it off and every agent command refuses.
- **Offline mode**: only a server on your own computer, for example Ollama, may be contacted.
- **Per project:** a project can switch the agent off for itself with `enabled = false` in `.manang/agent/policy.toml`. A project can only tighten the rules, never loosen them.
- Under **Settings > Plugins > Agent**, remove the key under **Key** and withdraw consent under **Privacy**.
