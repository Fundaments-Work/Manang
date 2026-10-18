/**
 * The `[plugin]`, `[permissions]` and `[[contributes.settings]]` tables of the
 * first-party plugins' `manifest.toml`, for the in-memory kernel. Generated
 * by `scripts/gen-mock-manifests.py`; regenerate when a manifest changes
 * rather than editing by hand.
 */

import type { SettingDef } from "./api";
import type { PluginPermissions } from "./client";

export interface MockManifest {
  readonly description: string;
  readonly permissions: PluginPermissions;
  /** `[plugin] settings_contexts`: where its Settings section belongs. */
  readonly settingsContexts?: readonly string[];
  readonly settings?: readonly Omit<SettingDef, "owner" | "category">[];
}

export const MOCK_MANIFESTS: Readonly<Record<string, MockManifest>> = {
  agent: {
    description:
      "AI agent: chat, rewrite, explain and fix with your own provider key. Writes are proposals you accept.",
    permissions: {},
    settingsContexts: ["app", "project", "document"],
    settings: [
      {
        id: "agent.enabled",
        type: "bool",
        default: true,
        title: "Enable the AI agent",
        description:
          "Nothing is sent anywhere until you pick a provider, give your consent and press Send. A project can switch the agent off for itself.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "agent.provider",
        type: "enum",
        values: [
          "anthropic",
          "openai",
          "deepseek",
          "openrouter",
          "groq",
          "ollama",
          "llama_cpp",
          "lm_studio",
          "custom",
        ],
        default: "anthropic",
        title: "Provider",
        description: "Who answers your requests. Set the key under Key.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "agent.model",
        type: "string",
        default: "",
        title: "Model",
        description: "Model id. Empty uses the provider's first listed model.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "agent.base-url",
        type: "string",
        default: "",
        title: "Base URL",
        description:
          "Empty uses the provider's address. Set it for a custom or local server.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "agent.client",
        type: "enum",
        values: ["rig", "builtin"],
        default: "rig",
        title: "Provider client",
        description:
          "rig is the default client layer; builtin is Manang's own, kept as a fallback.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "agent.offline",
        type: "bool",
        default: false,
        title: "Offline mode",
        description: "Only providers on this machine (localhost) may be contacted.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "agent.include-thinking",
        type: "bool",
        default: false,
        title: "Show the model's reasoning",
        description:
          "Shows the model's reasoning text dimmed in the Agent view when the provider streams it.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "agent.max-context-tokens",
        type: "int",
        min: 1000,
        max: 1000000,
        default: 32000,
        title: "Context limit (tokens)",
        description:
          "A request whose attached context is larger than this is refused before it is sent. About 3.5 characters per token.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "agent.redact-emails",
        type: "bool",
        default: false,
        title: "Redact email addresses",
        description:
          "Replace email addresses in what is sent. Secret-like strings are always redacted.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "agent.redact-paths",
        type: "bool",
        default: false,
        title: "Redact user names in paths",
        description: "Replace the user name in home-directory paths in what is sent.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "agent.retention-days",
        type: "int",
        min: 0,
        max: 3650,
        default: 30,
        title: "Keep sessions (days)",
        description:
          "Sessions older than this are deleted when a new one starts. 0 keeps them.",
        scope: "user",
        "requires-restart": false,
      },
    ],
  },
  citations: {
    description:
      "Bibliography management, CSL-JSON & BibTeX index, formatting, and Crossref/OpenAlex/arXiv verification",
    permissions: {
      network: ["api.crossref.org", "api.openalex.org", "export.arxiv.org"],
      filesystem: ["read:project"],
    },
    settingsContexts: ["document"],
    settings: [
      {
        id: "citations.style",
        type: "enum",
        values: ["apa", "numeric"],
        default: "apa",
        scope: "project",
        title: "Citation style",
        description:
          "Style used to format a reference when none is requested: `apa` (author-year) or `numeric` (IEEE-like).",
        "requires-restart": false,
      },
    ],
  },
  diagram: {
    description:
      "Text diagrams (Mermaid, Pikchr, ASCII) to deterministic SVG. Diagnostics use 1-based lines and 1-based columns counted in characters, end exclusive, like the Typst plugin.",
    permissions: {},
    settingsContexts: ["diagram"],
  },
  editor: {
    description: "Document store, edits and the active-buffer service",
    permissions: { buffer: ["write:active"] },
  },
  figure: {
    description:
      "Typed SVG scene graph, deterministic headless renderer and vector studio",
    permissions: { filesystem: ["write:project"] },
    settingsContexts: ["figure"],
  },
  git: {
    description: "Git status, diffs, history and safe write operations",
    permissions: { filesystem: ["write:project"] },
  },
  heal: {
    description:
      "Self-healing: deterministic one-click fixes for small compile errors (a missing package, an unclosed delimiter)",
    permissions: {},
    settingsContexts: ["document"],
    settings: [
      {
        id: "heal.enabled",
        type: "bool",
        default: true,
        title: "Offer one-click fixes",
        description:
          "Show a Fix button on errors that have a mechanical fix: a missing package, an unclosed delimiter.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "heal.auto-apply",
        type: "bool",
        default: false,
        title: "Apply certain fixes on save",
        description:
          "When you save, apply the fixes that have exactly one reading (such as a missing \\usepackage) and say so. Off by default.",
        scope: "user",
        "requires-restart": false,
      },
    ],
  },
  latex: {
    description:
      "LaTeX compilation with the system TeX (optional Tectonic), structured diagnostics and SyncTeX",
    permissions: { filesystem: ["read:project", "write:cache"] },
    settingsContexts: ["engine:latex"],
    settings: [
      {
        id: "latex.backend",
        type: "enum",
        values: ["auto", "system", "tectonic"],
        default: "auto",
        title: "Backend",
        description:
          "What compiles LaTeX. `auto` uses the system TeX installation when one is found, otherwise the optional built-in Tectonic engine.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "latex.engine",
        type: "enum",
        values: ["pdflatex", "xelatex", "lualatex"],
        default: "pdflatex",
        scope: "project",
        title: "Engine",
        description:
          "TeX engine used to compile LaTeX documents with the system backend, unless a compile request names one.",
        "requires-restart": false,
      },
      {
        id: "latex.latexmk",
        type: "bool",
        default: true,
        title: "Use latexmk",
        description:
          "Drive the system backend with latexmk when it is installed, so bibliographies and cross-references settle automatically.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "latex.timeout-seconds",
        type: "int",
        min: 5,
        max: 3600,
        default: 60,
        title: "Compile timeout (seconds)",
        description:
          "Wall-clock limit for one compile; a runaway document is stopped after this many seconds.",
        scope: "user",
        "requires-restart": false,
      },
    ],
  },
  live: {
    description:
      "Edit one file together in real time. End-to-end encrypted through a relay you choose; the file on disk stays plain text.",
    permissions: {},
    settings: [
      {
        id: "live.relay-url",
        type: "string",
        default: "",
        title: "Relay address",
        description:
          "Where live sessions meet, for example wss://relay.example.org. The relay forwards encrypted data and cannot read your text. Empty turns sharing off; joining uses the address in the invite.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "live.display-name",
        type: "string",
        default: "",
        title: "Name shown to collaborators",
        description: "Shown next to your caret. Empty uses your system user name.",
        scope: "user",
        "requires-restart": false,
      },
    ],
  },
  markdown: {
    description:
      "Markdown rendering, outline extraction, XSS sanitization, and Typst conversion",
    permissions: {},
    settingsContexts: ["engine:markdown"],
  },
  plugins: {
    description: "Install, approve, enable, disable and remove plugins",
    permissions: {},
  },
  project: {
    description:
      "Project config, file tree, confined file operations, watching and git status",
    permissions: { filesystem: ["write:project"] },
  },
  recovery: {
    description:
      "Crash-safe snapshots of unsaved edits, offered back after a crash or force-quit",
    permissions: {},
  },
  search: {
    description:
      "Fuzzy file finder, project-wide text search, atomic replace and in-document find",
    permissions: { filesystem: ["write:project"] },
    settingsContexts: ["app", "project"],
    settings: [
      {
        id: "search.exclude",
        type: "list",
        item: { type: "string" },
        default: [],
        scope: "project",
        title: "Exclude globs",
        description:
          "Glob patterns left out of project text search and replace, in addition to each search's own excludes and .gitignore (for example `build/` or `*.log`).",
        "requires-restart": false,
      },
      {
        id: "search.include-hidden",
        type: "bool",
        default: false,
        title: "Include hidden files",
        description:
          "Search dotfiles and dot folders when a search does not say otherwise. `.git` and `.manang` are always skipped.",
        scope: "user",
        "requires-restart": false,
      },
    ],
  },
  settings: {
    description:
      "Typed settings registry, layered user/project values, keybindings and themes",
    permissions: { filesystem: ["write:config", "write:project"] },
  },
  typst: {
    description:
      "Typst compiler: incremental compile, SVG page render, PDF export and source/preview jumps",
    permissions: { filesystem: ["read:project"] },
    settingsContexts: ["engine:typst"],
  },
  writing: {
    description:
      "Writing tools: formatting, spell check, prose lint, symbols, snippets and statistics",
    permissions: { filesystem: ["read:config", "write:config"] },
    settingsContexts: ["document"],
    settings: [
      {
        id: "writing.spellcheck",
        type: "bool",
        default: true,
        title: "Spell check",
        description:
          "Report misspelled words. When off, spell check and prose lint report no spelling problems.",
        scope: "user",
        "requires-restart": false,
      },
      {
        id: "writing.spell-language",
        type: "enum",
        values: ["en_US", "en_GB"],
        default: "en_US",
        scope: "project",
        title: "Spelling language",
        description: "Dictionary used to check spelling.",
        "requires-restart": false,
      },
      {
        id: "writing.format-width",
        type: "int",
        min: 20,
        max: 1000,
        default: 80,
        scope: "project",
        title: "Format line width",
        description: "Line width the Typst formatter aims for.",
        "requires-restart": false,
      },
    ],
  },
};
