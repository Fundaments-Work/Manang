import { describe, expect, it } from "vitest";

import { isKernelError } from "./client";
import { createMockKernel } from "./mock";

describe("mock kernel", () => {
  it("lists the editor commands from the manifest", async () => {
    const ids = (await createMockKernel().commands()).map((c) => c.id);

    expect(ids).toEqual([
      "editor.activate",
      "editor.delete",
      "editor.info",
      "editor.insert",
      "editor.new",
      "editor.open",
      "editor.save",
      "editor.text",
    ]);
  });

  it("indexes by Unicode scalar like ropey, and bumps the revision per edit", async () => {
    const k = createMockKernel();
    const { id } = await k.run<{ id: number }>("editor.new");

    await expect(k.run("editor.insert", { id, at: 0, text: "a😀b" })).resolves.toEqual({
      revision: 1,
    });
    // 😀 is one scalar: index 2 sits between it and "b".
    await k.run("editor.insert", { id, at: 2, text: "-" });
    await expect(k.run("editor.text", { id })).resolves.toBe("a😀-b");
    await expect(k.run("editor.delete", { id, from: 1, to: 2 })).resolves.toEqual({
      revision: 3,
    });
    await expect(k.run("editor.info", {})).resolves.toMatchObject({
      id,
      chars: 3,
      lines: 1,
      revision: 3,
      dirty: true,
    });
  });

  it("rejects out-of-range edits with a structured error", async () => {
    const k = createMockKernel();
    await k.run("editor.new");

    const error: unknown = await k
      .run("editor.delete", { from: 0, to: 4 })
      .catch((e: unknown) => e);

    expect(isKernelError(error)).toBe(true);
    expect((error as { message: string }).message).toContain("outside the document");
  });

  it("requires an active document and an unsigned `at`", async () => {
    const k = createMockKernel();
    await expect(k.run("editor.text")).rejects.toMatchObject({
      message: "no document is active",
    });
    await k.run("editor.new");
    await expect(k.run("editor.insert", { at: -1, text: "x" })).rejects.toMatchObject({
      message: "`at` must be an unsigned integer",
    });
  });

  it("opens, saves and clears the dirty flag", async () => {
    const k = createMockKernel({ files: { "a.typ": "hi" } });
    const { id } = await k.run<{ id: number }>("editor.open", { path: "a.typ" });
    await k.run("editor.insert", { id, at: 2, text: "!" });
    await k.run("editor.save", { id });

    await expect(k.run("editor.info", { id })).resolves.toMatchObject({ dirty: false });
    await expect(k.run("editor.open", { path: "nope" })).rejects.toBeDefined();
  });

  it("rejects unknown commands and exposes extra handlers", async () => {
    const k = createMockKernel({
      handlers: { "lint.diagnostics": () => [] },
      binaryHandlers: { "typst.render": () => new Uint8Array([1]) },
    });

    await expect(k.run("nope")).rejects.toMatchObject({
      message: "unknown command `nope`",
    });
    await expect(k.run("lint.diagnostics")).resolves.toEqual([]);
    await expect(k.runBinary("typst.render")).resolves.toEqual(new Uint8Array([1]));
    expect((await k.commands()).map((c) => c.id)).toContain("typst.render");
  });
});
