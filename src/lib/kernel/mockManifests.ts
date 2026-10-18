/**
 * The `[plugin]`, `[permissions]` and `[[contributes.settings]]` tables of
 * the first-party plugins' `manifest.toml`, for the in-memory kernel. Keep in
 * step with `crates/manang-<name>/manifest.toml`.
 */

import type { SettingDef } from "./api";
import type { PluginPermissions } from "./client";

export interface MockManifest {
  readonly description: string;
  readonly permissions: PluginPermissions;
  readonly settings?: readonly Omit<SettingDef, "owner" | "category">[];
}

export const MOCK_MANIFESTS: Readonly<Record<string, MockManifest>> = {
  citations: {
    description:
      "Bibliography management, CSL-JSON & BibTeX index, formatting, and Crossref/OpenAlex/arXiv verification",
    permissions: {
      network: ["api.crossref.org", "api.openalex.org", "export.arxiv.org"],
      filesystem: ["read:project"],
    },
  },
  figure: {
    description:
      "Typed SVG scene graph, deterministic headless renderer and vector studio",
    permissions: { filesystem: ["write:project"] },
  },
  git: {
    description: "Git status, diffs, history and safe write operations",
    permissions: { filesystem: ["write:project"] },
  },
  latex: {
    description:
      "LaTeX compilation with the system TeX (optional Tectonic), structured diagnostics and SyncTeX",
    permissions: { filesystem: ["read:project", "write:cache"] },
    settings: [
      {
        id: "latex.backend",
        title: "Compiler",
        type: "enum",
        values: ["auto", "system", "tectonic"],
        default: "auto",
        scope: "user",
        requires_restart: false,
        description:
          "Which TeX compiles LaTeX documents. Auto uses the TeX installed on this computer and falls back to Tectonic.",
      },
      {
        id: "latex.engine",
        title: "Engine",
        type: "enum",
        values: ["pdflatex", "xelatex", "lualatex"],
        default: "pdflatex",
        scope: "project",
        requires_restart: false,
        description:
          "TeX engine for this project. XeLaTeX and LuaLaTeX can use system fonts.",
      },
      {
        id: "latex.latexmk",
        title: "Use latexmk",
        type: "bool",
        default: true,
        scope: "project",
        requires_restart: false,
        description:
          "Let latexmk decide how many passes and bibliography runs a document needs.",
      },
      {
        id: "latex.timeout-seconds",
        title: "Time limit",
        type: "int",
        min: 5,
        max: 600,
        default: 120,
        scope: "user",
        requires_restart: false,
        description: "Stop a compile that runs longer than this many seconds.",
      },
    ],
  },
  markdown: {
    description:
      "Markdown rendering, outline extraction, XSS sanitization, and Typst conversion",
    permissions: {},
  },
  project: {
    description:
      "Project config, file tree, confined file operations, watching and git status",
    permissions: { filesystem: ["write:project"] },
  },
  search: {
    description:
      "Fuzzy file finder, project-wide text search, atomic replace and in-document find",
    permissions: { filesystem: ["write:project"] },
    settings: [
      {
        id: "search.exclude",
        title: "Exclude",
        type: "list",
        item: { type: "string" },
        default: [],
        scope: "project",
        requires_restart: false,
        description: "Glob patterns of files that search and replace always skip.",
      },
      {
        id: "search.include-hidden",
        title: "Include hidden files",
        type: "bool",
        default: false,
        scope: "user",
        requires_restart: false,
        description: "Search files and folders whose names start with a dot.",
      },
    ],
  },
  settings: {
    description:
      "Typed settings registry, layered user/project values, keybindings and themes",
    permissions: { filesystem: ["write:config", "write:project"] },
  },
  typst: {
    description: "Typst compiler: incremental compile, SVG page render and PDF export",
    permissions: { filesystem: ["read:project"] },
  },
  writing: {
    description:
      "Writing tools: formatting, spell check, prose lint, symbols, snippets and statistics",
    permissions: { filesystem: ["read:config", "write:config"] },
    settings: [
      {
        id: "writing.spellcheck",
        title: "Check spelling",
        type: "bool",
        default: true,
        scope: "user",
        requires_restart: false,
        description: "Underline misspelled words as you type.",
      },
      {
        id: "writing.spell-language",
        title: "Spelling language",
        type: "enum",
        values: ["en-US", "en-GB"],
        default: "en-US",
        scope: "project",
        requires_restart: false,
        description: "Dictionary used to check spelling in this project.",
      },
    ],
  },
};
