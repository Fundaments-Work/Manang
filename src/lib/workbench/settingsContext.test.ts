import { describe, expect, it } from "vitest";

import type { SettingDef } from "$lib/kernel/api";
import { CATEGORY_CONTEXTS } from "$lib/kernel/mockSettings";
import { MOCK_MANIFESTS } from "$lib/kernel/mockManifests";
import {
  APP_ORIGIN,
  contextsOf,
  defaultSettingsScope,
  groupsForContext,
  inContext,
  pluginContexts,
  scopeLabel,
  settingsOriginFor,
  splitMatches,
  type SettingsOrigin,
} from "./settings";

function def(id: string, owner: string, contexts?: readonly string[]): SettingDef {
  return {
    id,
    type: "bool",
    default: false,
    description: `The ${id} setting.`,
    scope: "project",
    "requires-restart": false,
    owner,
    category: id.split(".")[0] ?? "",
    contexts,
  };
}

/** One setting per group, with the contexts the real registry declares. */
const DEFS: SettingDef[] = [
  ...Object.entries(CATEGORY_CONTEXTS).map(([category, contexts]) =>
    def(`${category}.sample`, "core", contexts),
  ),
  ...Object.entries(MOCK_MANIFESTS)
    .filter(([, m]) => m.settings !== undefined)
    .map(([name, m]) => def(`${name}.sample`, name, m.settingsContexts)),
  def("figure.sample", "figure", MOCK_MANIFESTS["figure"]?.settingsContexts),
];

const typst: SettingsOrigin = {
  kind: "document",
  path: "paper/main.typ",
  engine: "typst",
};
const latex: SettingsOrigin = { kind: "document", path: "thesis.tex", engine: "latex" };
const figure: SettingsOrigin = { kind: "figure", path: "figures/plot.figure.json" };

const ids = (origin: SettingsOrigin): string[] =>
  groupsForContext(DEFS, origin).map((g) => g.id);

describe("contextual settings", () => {
  it("shows a Typst document's groups only", () => {
    expect(ids(typst)).toEqual([
      "editor",
      "preview",
      "compile",
      "files",
      "agent",
      "citations",
      "heal",
      "writing",
    ]);
    expect(ids(typst)).not.toContain("appearance");
    expect(ids(typst)).not.toContain("latex");
    expect(ids(typst)).not.toContain("search");
  });

  it("shows the engine's plugin for its documents", () => {
    expect(ids(latex)).toContain("latex");
    expect(ids(typst)).not.toContain("latex");
  });

  it("shows everything in the app scope", () => {
    expect(ids(APP_ORIGIN)).toEqual([
      "appearance",
      "editor",
      "preview",
      "compile",
      "files",
      "agent",
      "citations",
      "figure",
      "heal",
      "latex",
      "live",
      "search",
      "writing",
    ]);
  });

  it("scopes the figure, PDF and diagram editors", () => {
    expect(ids(figure)).toEqual(["figure"]);
    expect(ids({ kind: "pdf", path: null })).toEqual(["preview"]);
    expect(ids({ kind: "diagram", path: "flow.mmd" })).toEqual(["editor"]);
  });

  it("treats undeclared contexts as app", () => {
    expect(contextsOf(def("x.y", "x"))).toEqual(["app"]);
    expect(contextsOf(def("x.y", "x", []))).toEqual(["app"]);
    expect(inContext(["app"], typst)).toBe(false);
    expect(inContext(["app"], APP_ORIGIN)).toBe(true);
  });

  it("takes a plugin's contexts from its manifest, else its settings", () => {
    const info = [
      { name: "typst", contexts: ["engine:typst"] },
      { name: "git", contexts: [] },
    ];
    expect(pluginContexts("typst", info, [])).toEqual(["engine:typst"]);
    expect(pluginContexts("git", info, [])).toEqual(["app"]);
    expect(pluginContexts("latex", null, DEFS)).toEqual(["engine:latex"]);
    expect(pluginContexts("unknown", null, DEFS)).toEqual(["app"]);
  });

  it("splits a search into this scope and elsewhere", () => {
    const { here, elsewhere } = splitMatches(DEFS, "sample", typst);
    expect(here.map((d) => d.category)).toEqual([
      "editor",
      "preview",
      "compile",
      "files",
      "agent",
      "citations",
      "heal",
      "writing",
    ]);
    expect(elsewhere.map((d) => d.category).sort()).toEqual(
      ["appearance", "figure", "latex", "live", "search"].sort(),
    );
    expect(splitMatches(DEFS, "sample", APP_ORIGIN).elsewhere).toEqual([]);
    expect(splitMatches(DEFS, "appearance", typst)).toEqual({
      here: [],
      elsewhere: [DEFS[0]],
    });
  });

  it("defaults to This project when the project overrides a shown setting", () => {
    const shown = groupsForContext(DEFS, typst).flatMap((g) => g.settings);
    const none = (): string => "default";
    const editorFromProject = (id: string): string =>
      id === "editor.sample" ? "project" : "user";
    expect(defaultSettingsScope(typst, shown, none, true)).toBe("user");
    expect(defaultSettingsScope(typst, shown, editorFromProject, true)).toBe("project");
    expect(defaultSettingsScope(typst, shown, editorFromProject, false)).toBe("user");
    expect(defaultSettingsScope(APP_ORIGIN, shown, editorFromProject, true)).toBe(
      "user",
    );
    // An override of a setting the scope does not show does not count.
    const appearanceFromProject = (id: string): string =>
      id === "appearance.sample" ? "project" : "default";
    expect(defaultSettingsScope(typst, shown, appearanceFromProject, true)).toBe(
      "user",
    );
  });

  it("names the scope in one line", () => {
    expect(scopeLabel(typst)).toBe("Settings for main.typ · Typst");
    expect(scopeLabel(latex)).toBe("Settings for thesis.tex · LaTeX");
    expect(scopeLabel(figure)).toBe("Settings for plot.figure.json · Figure");
    expect(scopeLabel({ kind: "pdf", path: null })).toBe("Settings for the PDF viewer");
    expect(scopeLabel(APP_ORIGIN)).toBe("All settings");
  });

  it("derives the origin from the tab Settings opens over", () => {
    expect(settingsOriginFor({ kind: "document", path: "a/main.typ" }, false)).toEqual({
      kind: "document",
      path: "a/main.typ",
      engine: "typst",
    });
    expect(settingsOriginFor({ kind: "document", path: "main.typ" }, true)).toBe(
      APP_ORIGIN,
    );
    expect(settingsOriginFor(undefined, false)).toBe(APP_ORIGIN);
    expect(settingsOriginFor({ kind: "figure", path: "f.figure.json" }, false)).toEqual(
      { kind: "figure", path: "f.figure.json" },
    );
    expect(settingsOriginFor({ kind: "viewer", path: "out/main.pdf" }, false)).toEqual({
      kind: "pdf",
      path: "out/main.pdf",
    });
    expect(settingsOriginFor({ kind: "viewer", path: "photo.png" }, false)).toBe(
      APP_ORIGIN,
    );
    expect(settingsOriginFor({ kind: "document", path: "flow.mmd" }, false)).toEqual({
      kind: "diagram",
      path: "flow.mmd",
    });
    expect(settingsOriginFor({ kind: "document", path: "notes.txt" }, false)).toEqual({
      kind: "document",
      path: "notes.txt",
      engine: null,
    });
  });
});
