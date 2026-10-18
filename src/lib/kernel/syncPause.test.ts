import { describe, expect, it } from "vitest";

import { createMockKernel } from "./mock";
import { KernelSync } from "./sync";

async function rig(text: string) {
  const mock = createMockKernel();
  const sync = new KernelSync(mock, { applyText: () => undefined });
  await sync.start(text);
  return { mock, sync, kernelText: () => mock.run<string>("editor.text", {}) };
}

describe("KernelSync while a live session keeps the kernel copy", () => {
  it("sends nothing while paused", async () => {
    const { sync, kernelText } = await rig("abc");
    sync.pause();
    expect(sync.paused).toBe(true);
    sync.submit([{ kind: "insert", at: 3, text: "!" }]);
    await sync.idle();
    expect(await kernelText()).toBe("abc");
    await sync.check();
    await sync.resync();
    expect(sync.resyncCount).toBe(0);
  });

  it("on resume, brings the kernel copy to the editor's text with the smallest edit", async () => {
    const { sync, mock, kernelText } = await rig("héllo 😀 world");
    sync.pause();
    // The session changed the kernel copy behind the shell's back.
    await mock.run("editor.insert", { id: 0, at: 0, text: ">> " });
    await sync.resume(">> héllo 😀 brave world");
    expect(sync.paused).toBe(false);
    expect(await kernelText()).toBe(">> héllo 😀 brave world");
    expect(sync.status).toBe("synced");
    // Sending works again, with a revision it agrees on.
    sync.submit([{ kind: "insert", at: 0, text: "x" }]);
    await sync.idle();
    expect(await kernelText()).toBe("x>> héllo 😀 brave world");
    expect(sync.resyncCount).toBe(0);
  });

  it("resumes with nothing to do when the copies agree", async () => {
    const { sync, kernelText } = await rig("same");
    sync.pause();
    await sync.resume("same");
    expect(await kernelText()).toBe("same");
    expect(sync.paused).toBe(false);
  });
});
