# Using the AI agent (private beta)

The agent helps you rewrite, shorten, translate and explain text, fix compile problems, and
chat about your document. It uses **your own key** with a provider you choose. Nothing is sent
until you press Send (or pick an action), and every change it suggests is a proposal you accept
or reject.

## Set up with DeepSeek

1. Create a key at <https://platform.deepseek.com> (API keys). Add a little balance.
2. In Manang open **Settings** (Ctrl+,) and find the **Agent** section (or open the Agent tab
   in the bottom panel and press **Set up**).
3. **Provider**: choose `deepseek`. **Model**: `deepseek-flash` (fast, cheap) or
   `deepseek-v4-pro` (stronger). These are the names on api-docs.deepseek.com as of
   2026-10-18; the old `deepseek-v4-flash` still works but is retired. Leave **Base URL**
   empty.
4. Under **Provider and key** paste the key and press **Save key**. It goes to your system
   keychain and is never shown again. The panel says where it is held; "Not saved: stored for
   this session only" means no keychain was available and you must paste it again next time.
5. Press **Test connection**. A friendly message tells you if the key, balance or address is
   wrong.
6. Open a document, select a paragraph, then **Edit, AI, Rewrite selection** (or Ctrl+K and
   type "AI"). The first time, a dialog states what is sent and where; press **Agree**.
7. The result appears in the editor as an inline diff: removed text struck through, new text
   in green. **Accept** applies it as one edit (Ctrl+Z undoes it); **Reject** or Esc keeps your
   text.

## What is sent

Only your message plus the chips in the **Context** tray above the input (default: the
selection). Each chip shows its size; **Preview what will be sent** shows the exact request
before anything leaves your machine, including secret-like strings that were redacted. The
document is sent as delimited data, never as instructions. DeepSeek's reasoning text is kept
in the session record and shown dimmed only if you enable "Show the model's reasoning".

Costs shown are estimates from published prices; the provider's bill is the truth.

## Fix with AI

In the Problems list, an error with a file shows **Fix with AI**. It sends the diagnostic and
the lines around it, and proposes an edit as an inline diff. After you accept, the normal
compile runs; Manang does not yet check automatically that the fix worked.

## Where things are stored

`.manang/agent/sessions/<id>/` in your project (`transcript.jsonl`, `audit.jsonl`,
`meta.toml`), excluded from git by `.manang/agent/.gitignore`. Old sessions are deleted after
the retention setting (30 days). No key is ever written there.

## Turning it off

* Settings, Agent, **Enable the AI agent** off: every agent command refuses.
* **Offline mode**: only a server on this machine (for example Ollama) is allowed.
* A project can switch it off for itself with `.manang/agent/policy.toml`:
  `enabled = false` (a project can only tighten, never loosen).
* Remove the key under Provider and key, and withdraw consent there.

## Browser build

`pnpm dev` uses a clearly labelled **mock provider** that streams canned text and a placeholder
edit, so the interface can be tried without a key. The desktop app talks to the real provider.
