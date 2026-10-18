import { describe, expect, it } from "vitest";

import { createDevKernel } from "./dev";
import { pluginsApi, type PluginAccess, type PluginState } from "./pluginsApi";
import { MOCK_PLUGIN_SOURCES } from "./mockPlugins";
import type { KernelEvent } from "./client";

const WORD_STATS = Object.keys(MOCK_PLUGIN_SOURCES)[0] ?? "";

function rig() {
  const kernel = createDevKernel();
  const api = pluginsApi(kernel);
  const events: KernelEvent[] = [];
  void kernel.onEvent((e) => events.push(e));
  const find = async (name: string): Promise<PluginState | undefined> =>
    (await api.list()).plugins.find((p) => p.name === name);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 5));
  return { kernel, api, events, find, settle };
}

const message = (e: unknown): string => (e as { message: string }).message;

describe("the mock plugin manager", () => {
  it("lists built-ins and one sample in every state", async () => {
    const { api } = rig();
    const { plugins } = await api.list();
    const byName = new Map(plugins.map((p) => [p.name, p]));
    expect(byName.get("editor")).toMatchObject({ tier: "native", core: true });
    expect(byName.get("git")).toMatchObject({ tier: "native", core: false });
    expect(byName.get("plugins")?.commands.length).toBeGreaterThan(5);
    expect(byName.get("word-count")?.status).toBe("loaded");
    expect(byName.get("latex-lint")?.status).toBe("failed");
    expect(byName.get("latex-lint")?.error).toMatch(/imports/);
    expect(byName.get("citation-fetch")?.status).toBe("needs_approval");
    expect(byName.get("slide-themes")?.status).toBe("disabled");
    const update = byName.get("plot-export");
    expect(update).toMatchObject({ status: "needs_approval", update_pending: true });
    expect(update?.new_permissions.network).toEqual(["plots.example.org"]);
    expect(update?.running_version).toBe("0.2.0");
  });

  it("refuses to disable or remove core plugins, with a coded error", async () => {
    const { api } = rig();
    await expect(api.disable("editor")).rejects.toSatisfy((e) =>
      message(e).startsWith("[core_plugin]"),
    );
    await expect(api.uninstall("settings")).rejects.toSatisfy((e) =>
      message(e).startsWith("[core_plugin]"),
    );
    await expect(api.uninstall("git")).rejects.toSatisfy((e) =>
      message(e).startsWith("[built_in]"),
    );
    await expect(api.enable("../x")).rejects.toSatisfy((e) =>
      message(e).startsWith("[invalid_name]"),
    );
    await expect(api.enable("nope")).rejects.toSatisfy((e) =>
      message(e).startsWith("[not_installed]"),
    );
  });

  it("switches a built-in off and on, and an installed plugin back to where it was", async () => {
    const { api, find } = rig();
    expect((await api.disable("git")).status).toBe("disabled");
    expect((await find("git"))?.enabled).toBe(false);
    expect((await api.enable("git")).status).toBe("loaded");
    expect((await api.disable("word-count")).status).toBe("disabled");
    expect((await api.enable("word-count")).status).toBe("loaded");
    // A plugin that waits for approval is still waiting after off and on.
    await api.disable("citation-fetch");
    expect((await api.enable("citation-fetch")).status).toBe("needs_approval");
  });

  it("installs, holds for approval, then loads on approve", async () => {
    const { api, find, events, settle } = rig();
    const report = await api.inspect(WORD_STATS);
    expect(report).toMatchObject({ name: "word-stats", kind: "archive" });
    expect(await find("word-stats")).toBeUndefined();

    const waiting = await api.install(WORD_STATS);
    expect(waiting.status).toBe("needs_approval");
    expect(waiting.new_permissions.buffer).toEqual(["read:active"]);

    const tooMuch: PluginAccess = {
      filesystem: [],
      network: ["evil.example"],
      buffer: [],
    };
    await expect(api.approve("word-stats", tooMuch)).rejects.toSatisfy((e) =>
      message(e).startsWith("[permission_mismatch]"),
    );
    const ready = await api.approve("word-stats", report.permissions);
    expect(ready).toMatchObject({ status: "loaded", running_version: "0.1.0" });
    expect(ready.granted.buffer).toEqual(["read:active"]);

    await expect(api.install(WORD_STATS)).rejects.toSatisfy((e) =>
      message(e).startsWith("[already_installed]"),
    );
    await settle();
    const topics = events.map((e) => e.topic);
    expect(topics).toContain("plugins.needs-approval");
    expect(topics).toContain("plugins.loaded");
    expect(topics).toContain("plugins.state-changed");
  });

  it("approves an update that asked for more, and uninstalls with a trash path", async () => {
    const { api, find } = rig();
    const update = await find("plot-export");
    expect(update).toBeDefined();
    const done = await api.approve("plot-export", update?.permissions as PluginAccess);
    expect(done).toMatchObject({
      status: "loaded",
      running_version: "0.3.0",
      update_pending: false,
    });
    const gone = await api.uninstall("plot-export");
    expect(gone.trashed_to).toContain("plugins-trash");
    expect(await find("plot-export")).toBeUndefined();
  });

  it("keeps a failed plugin failed on reload and reports it as an event", async () => {
    const { api, events, settle } = rig();
    const state = await api.reload("latex-lint");
    expect(state.status).toBe("failed");
    await settle();
    const failed = events.find((e) => e.topic === "plugins.failed");
    expect(failed?.payload).toMatchObject({ name: "latex-lint" });
    expect((failed?.payload as { error: string }).error).not.toBe("");
  });

  it("rejects paths it does not know", async () => {
    const { api } = rig();
    await expect(api.inspect("/nope")).rejects.toSatisfy((e) =>
      message(e).startsWith("[invalid_path]"),
    );
  });
});
