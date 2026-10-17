import { describe, expect, it, vi } from "vitest";

import { ArtifactUrl, sniffArtifact } from "./artifact";
import { createMockKernel } from "./mock";
import { pickPreviewCommand, renderPreview } from "./preview";

const enc = (s: string): Uint8Array => new TextEncoder().encode(s);

describe("sniffArtifact", () => {
  it("recognises PDF and SVG by content", () => {
    expect(sniffArtifact(enc("%PDF-1.7\n"))?.kind).toBe("pdf");
    expect(sniffArtifact(enc('<svg xmlns="http://www.w3.org/2000/svg"/>'))?.kind).toBe(
      "svg",
    );
    expect(sniffArtifact(enc('<?xml version="1.0"?>\n<svg/>'))?.mime).toBe(
      "image/svg+xml",
    );
    expect(sniffArtifact(enc("hello"))).toBeNull();
    expect(sniffArtifact(new Uint8Array())).toBeNull();
  });
});

describe("ArtifactUrl", () => {
  it("revokes the previous URL on replace and on release", () => {
    let n = 0;
    const api = { create: vi.fn(() => `blob:${++n}`), revoke: vi.fn() };
    const holder = new ArtifactUrl(api);

    expect(holder.set(new Uint8Array([1]), "image/svg+xml")).toBe("blob:1");
    expect(api.revoke).not.toHaveBeenCalled();
    expect(holder.set(new Uint8Array([2]), "image/svg+xml")).toBe("blob:2");
    expect(api.revoke).toHaveBeenCalledWith("blob:1");
    holder.release();
    expect(api.revoke).toHaveBeenLastCalledWith("blob:2");
    expect(holder.url).toBeNull();
    holder.release();
    expect(api.revoke).toHaveBeenCalledTimes(2);
  });

  it("builds a typed Blob", () => {
    let seen: Blob | undefined;
    const holder = new ArtifactUrl({
      create: (b) => ((seen = b), "blob:x"),
      revoke: () => undefined,
    });
    holder.set(new Uint8Array([37, 80]), "application/pdf");
    expect(seen?.type).toBe("application/pdf");
    expect(seen?.size).toBe(2);
  });
});

describe("preview commands", () => {
  const svg = enc("<svg/>");

  it("is empty when no producing command exists", async () => {
    const k = createMockKernel();
    const command = await pickPreviewCommand(k, ["typst.render", "latex.export-pdf"]);

    expect(command).toBeNull();
    await expect(renderPreview(k, command)).resolves.toEqual({ state: "empty" });
  });

  it("prefers the first available configured command", async () => {
    const k = createMockKernel({
      binaryHandlers: { "latex.export-pdf": () => enc("%PDF-1.4") },
    });

    await expect(
      pickPreviewCommand(k, ["typst.render", "latex.export-pdf"]),
    ).resolves.toBe("latex.export-pdf");
    await expect(pickPreviewCommand(k, ["custom.render"])).resolves.toBeNull();
  });

  it("returns the artefact bytes when the command succeeds", async () => {
    const k = createMockKernel({ binaryHandlers: { "typst.render": () => svg } });
    const result = await renderPreview(k, "typst.render", { id: 0 });

    expect(result).toMatchObject({ state: "ready", artifact: { kind: "svg" } });
  });

  it("reports failures and unrecognised output as errors", async () => {
    const failing = createMockKernel({
      binaryHandlers: {
        "typst.render": () => {
          throw { message: "compile failed" };
        },
      },
    });
    await expect(renderPreview(failing, "typst.render")).resolves.toEqual({
      state: "error",
      message: "compile failed",
    });

    const junk = createMockKernel({
      binaryHandlers: { "typst.render": () => enc("???") },
    });
    await expect(renderPreview(junk, "typst.render")).resolves.toMatchObject({
      state: "error",
    });
  });
});
