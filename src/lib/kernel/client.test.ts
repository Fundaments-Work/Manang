import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, listen } = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

import { isKernelError, kernel } from "./client";

describe("kernel client", () => {
  // A block body: a function returned from beforeEach is run as a teardown,
  // and `mockReset()` returns the mock, which would call `invoke` again.
  beforeEach(() => {
    invoke.mockReset();
  });

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

  it("forwards kernel://event payloads to the handler and returns an unlisten", async () => {
    const unlisten = vi.fn();
    listen.mockImplementation(
      (_name: string, cb: (m: { payload: unknown }) => void) => {
        cb({
          payload: { topic: "project.changed", payload: { root: "/p", changes: [] } },
        });
        return Promise.resolve(unlisten);
      },
    );
    const seen: unknown[] = [];

    const stop = await kernel.onEvent((event) => seen.push(event));

    expect(listen).toHaveBeenCalledWith("kernel://event", expect.any(Function));
    expect(seen).toEqual([
      { topic: "project.changed", payload: { root: "/p", changes: [] } },
    ]);
    stop();
    expect(unlisten).toHaveBeenCalled();
  });

  it("carries on without an event channel, and logs why", async () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => undefined);
    listen.mockRejectedValue(new Error("no tauri"));
    const stop = await kernel.onEvent(() => undefined);
    expect(() => stop()).not.toThrow();
    expect(logged).toHaveBeenCalledWith(
      "[manang] kernel events are unavailable:",
      expect.any(Error),
    );
    logged.mockRestore();
  });

  it("logs a failed command with its id and still rejects for the caller", async () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const failure = { message: "[not_found] no such file" };
    invoke.mockRejectedValue(failure);

    await expect(kernel.run("project.read", { path: "x" })).rejects.toBe(failure);
    logged.mockRestore();
  });

  it("recognises structured kernel errors", () => {
    expect(isKernelError({ message: "no such command" })).toBe(true);
    expect(isKernelError("boom")).toBe(false);
    expect(isKernelError(null)).toBe(false);
  });
});
