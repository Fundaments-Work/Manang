import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { isKernelError, kernel } from "./client";

describe("kernel client", () => {
  beforeEach(() => invoke.mockReset());

  it("lists commands through kernel_commands", async () => {
    invoke.mockResolvedValue([{ id: "editor.new", title: "New", plugin: "editor" }]);

    await expect(kernel.commands()).resolves.toHaveLength(1);
    expect(invoke).toHaveBeenCalledWith("kernel_commands");
  });

  it("dispatches commands by id with structured args", async () => {
    invoke.mockResolvedValue({ revision: 1 });

    await kernel.run("editor.insert", { at: 0, text: "x" });

    expect(invoke).toHaveBeenCalledWith("kernel_invoke", {
      command: "editor.insert",
      args: { at: 0, text: "x" },
    });
  });

  it("returns binary results as a Uint8Array, not base64", async () => {
    invoke.mockResolvedValue(new Uint8Array([1, 2, 3]).buffer);

    const bytes = await kernel.runBinary("typst.render");

    expect(bytes).toBeInstanceOf(Uint8Array);
    expect([...bytes]).toEqual([1, 2, 3]);
  });

  it("recognises structured kernel errors", () => {
    expect(isKernelError({ message: "no such command" })).toBe(true);
    expect(isKernelError("boom")).toBe(false);
    expect(isKernelError(null)).toBe(false);
  });
});
