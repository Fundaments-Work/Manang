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
  },
  editor: {
    description: "Document store, edits and the active-buffer service",
    permissions: { buffer: ["write:active"] },
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
  },
  writing: {
    description:
      "Writing tools: formatting, spell check, prose lint, symbols, snippets and statistics",
    permissions: { filesystem: ["read:config", "write:config"] },
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
