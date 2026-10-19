import { afterEach, describe, expect, it } from "vitest";

import { createDevKernel } from "./dev";
import type { BrowseResult, UpdateCheck } from "./marketApi";
import { pluginsApi } from "./pluginsApi";

function rig() {
  const kernel = createDevKernel();
  const api = pluginsApi(kernel);
  const find = async (name: string) =>
    (await api.list()).plugins.find((p) => p.name === name);
  return { kernel, api, find };
}

const message = (e: unknown): string => (e as { message: string }).message;
const flag = globalThis as Record<string, unknown>;

afterEach(() => {
  delete flag["__MANANG_MOCK_REGISTRY__"];
});

describe("the mock plugin registry", () => {
  it("lists the sample registry with install state and updates", async () => {
    const { kernel } = rig();
    const result = await kernel.run<BrowseResult>("plugins.browse", { refresh: true });
    expect(result.plugins.map((p) => p.id)).toEqual([
      "word-count",
      "slide-themes",
      "plot-export",
      "table-tools",
      "crossref-lookup",
    ]);
    const by = new Map(result.plugins.map((p) => [p.id, p]));
    expect(by.get("word-count")?.installed).toEqual({
      version: "0.1.0",
      update_available: true,
    });
    expect(by.get("table-tools")?.installed).toBeNull();
    // plot-export's newer version is already waiting for approval.
    expect(by.get("plot-export")?.installed?.update_available).toBe(false);
    expect(result.updates).toBe(2);
  });

  it("reports an update with and without a permission increase", async () => {
    const { kernel } = rig();
    const check = await kernel.run<UpdateCheck>("plugins.check-updates", {
      background: false,
    });
    const by = new Map(check.updates.map((u) => [u.id, u]));
    expect(by.get("word-count")?.permissions_grew).toBe(false);
    expect(by.get("slide-themes")?.permissions_grew).toBe(true);
    expect(by.get("slide-themes")?.new_permissions.network).toEqual([
      "fonts.example.org",
    ]);
  });

  it("installs only with consent, then records the registry as the source", async () => {
    const { kernel, api, find } = rig();
    await expect(
      kernel.run("plugins.install-from-registry", { id: "table-tools" }),
    ).rejects.toSatisfy((e) => message(e).startsWith("[consent_required]"));
    expect(await find("table-tools")).toBeUndefined();
    await kernel.run("plugins.install-from-registry", {
      id: "table-tools",
      consent: true,
    });
    const state = await find("table-tools");
    expect(state).toMatchObject({ status: "loaded", version: "1.0.0" });
    expect(state?.origin?.id).toBe("table-tools");
    expect(state?.granted.buffer).toEqual(["write:active"]);
    await api.uninstall("table-tools");
  });

  it("updates in place and clears the update", async () => {
    const { kernel, find } = rig();
    await kernel.run("plugins.install-from-registry", {
      id: "word-count",
      consent: true,
    });
    expect((await find("word-count"))?.running_version).toBe("0.2.0");
    const check = await kernel.run<UpdateCheck>("plugins.check-updates", {});
    expect(check.updates.map((u) => u.id)).toEqual(["slide-themes"]);
  });

  it("refuses a withdrawn version", async () => {
    const { kernel } = rig();
    await expect(
      kernel.run("plugins.install-from-registry", {
        id: "crossref-lookup",
        version: "0.4.1",
        consent: true,
      }),
    ).rejects.toSatisfy((e) => message(e).startsWith("[registry_withdrawn]"));
  });

  it("shows the registry's failures as coded errors", async () => {
    const { kernel } = rig();
    for (const [mode, code] of [
      ["signature", "registry_signature"],
      ["unreachable", "registry_unreachable"],
      ["off", "registry_off"],
    ] as const) {
      flag["__MANANG_MOCK_REGISTRY__"] = mode;
      await expect(kernel.run("plugins.browse", {})).rejects.toSatisfy((e) =>
        message(e).startsWith(`[${code}]`),
      );
    }
  });

  it("serves an offline list with its date", async () => {
    const { kernel } = rig();
    flag["__MANANG_MOCK_REGISTRY__"] = "offline";
    const result = await kernel.run<BrowseResult>("plugins.browse", {});
    expect(result.freshness.state).toBe("offline");
    expect(result.plugins.length).toBeGreaterThan(0);
  });
});
