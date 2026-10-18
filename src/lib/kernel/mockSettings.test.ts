import { describe, expect, it } from "vitest";
import { createDevKernel, DEMO_ROOT } from "./dev";
import { keybindingsApi, settingsApi, themeApi } from "./api";

describe("mockSettings plugin", () => {
  it("fetches settings schema and default values", async () => {
    const kernel = createDevKernel();
    const settings = settingsApi(kernel);

    const schema = await settings.schema();
    expect(schema.categories.length).toBeGreaterThan(0);
    expect(schema.settings.length).toBeGreaterThan(0);

    const fontSizeDef = schema.settings.find((s) => s.id === "editor.font-size");
    expect(fontSizeDef).toBeDefined();
    expect(fontSizeDef?.type).toBe("int");
    expect(fontSizeDef?.default).toBe(14);

    const all = await settings.get();
    expect(all.values["editor.font-size"]?.value).toBe(14);
    expect(all.values["editor.font-size"]?.source).toBe("default");
  });

  it("updates and resets user settings with validation", async () => {
    const kernel = createDevKernel();
    const settings = settingsApi(kernel);

    // Valid update
    const updated = await settings.set({ id: "editor.font-size", value: 16 });
    expect(updated.value).toBe(16);
    expect(updated.source).toBe("user");

    const fetched = await settings.get("editor.font-size");
    expect(fetched.value).toBe(16);

    // Reset
    const reset = await settings.reset({ id: "editor.font-size" });
    expect(reset.value).toBe(14);
    expect(reset.source).toBe("default");
  });

  it("handles project settings when project is open", async () => {
    const kernel = createDevKernel();
    const settings = settingsApi(kernel);

    await settings.useProject(DEMO_ROOT);
    const updated = await settings.set({
      id: "editor.font-size",
      value: 18,
      scope: "project",
    });
    expect(updated.value).toBe(18);
    expect(updated.source).toBe("project");
  });

  it("lists, sets and resets keybindings", async () => {
    const kernel = createDevKernel();
    const keybindings = keybindingsApi(kernel);

    const initial = await keybindings.list();
    expect(initial.bindings.length).toBeGreaterThan(0);

    // Override a keybinding
    await keybindings.set({ command: "file.save", key: "Mod+Alt+S" });
    const afterSet = await keybindings.list();
    const saveBinding = afterSet.bindings.find((b) => b.command === "file.save");
    expect(saveBinding?.key).toBe("Mod+Alt+S");
    expect(saveBinding?.source).toBe("user");

    // Reset override
    await keybindings.reset("file.save");
    const afterReset = await keybindings.list();
    const resetSave = afterReset.bindings.find((b) => b.command === "file.save");
    expect(resetSave?.key).toBe("Mod+S");
    expect(resetSave?.source).toBe("default");

    // A user binding on another command's key is reported.
    await keybindings.set({ command: "view.files", key: "Mod+S" });
    const shadowed = await keybindings.list();
    expect(shadowed.conflicts).toEqual([
      { key: "Ctrl+S", kind: "shadow", commands: ["file.save", "view.files"] },
    ]);
  });

  it("lists and gets themes", async () => {
    const kernel = createDevKernel();
    const theme = themeApi(kernel);

    const list = await theme.list();
    expect(list.themes.some((t) => t.id === "dark")).toBe(true);
    expect(list.themes.some((t) => t.id === "light")).toBe(true);

    const dark = await theme.get("dark");
    expect(dark.tokens["--mng-bg"]).toBe("#141416");

    const light = await theme.get("light");
    expect(light.tokens["--mng-bg"]).toBe("#faf9f7");
  });
});
