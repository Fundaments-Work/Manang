import { describe, expect, it } from "vitest";

import { decideExternal, externalMessage } from "./external";

const clean = { dirty: false, diskHash: "h1", savedText: "old", text: "old" };
const dirty = { dirty: true, diskHash: "h1", savedText: "old", text: "mine" };

describe("decideExternal", () => {
  it("ignores our own write and a touch that changed nothing", () => {
    expect(decideExternal(clean, { content: "old", hash: "h1" }).kind).toBe("ignore");
    expect(decideExternal(dirty, { content: "mine", hash: "h1" }).kind).toBe("ignore");
    // The hash lagged behind a save but the text is what we saved.
    expect(decideExternal(dirty, { content: "old", hash: "h2" }).kind).toBe("ignore");
  });

  it("reloads a clean document silently", () => {
    expect(decideExternal(clean, { content: "new", hash: "h2" }).kind).toBe("reload");
  });

  it("never overwrites a document with unsaved edits: it asks", () => {
    expect(decideExternal(dirty, { content: "new", hash: "h2" }).kind).toBe("conflict");
  });

  it("adopts the file when it now says exactly what the unsaved edits say", () => {
    expect(decideExternal(dirty, { content: "mine", hash: "h3" }).kind).toBe("adopt");
  });

  it("keeps the tab when the file is gone, dirty or not", () => {
    expect(decideExternal(clean, null).kind).toBe("deleted");
    expect(decideExternal(dirty, null).kind).toBe("deleted");
  });

  it("treats a file that was never on disk as changed once it appears", () => {
    const fresh = { dirty: true, diskHash: null, savedText: "", text: "typed" };
    expect(decideExternal(fresh, { content: "x", hash: "h9" }).kind).toBe("conflict");
  });
});

describe("externalMessage", () => {
  it("names the file and the state", () => {
    expect(externalMessage("main.typ", "changed", true)).toBe(
      "main.typ changed on disk.",
    );
    expect(externalMessage("a.typ", "deleted", true)).toContain("Your text is kept");
    expect(externalMessage("a.typ", "deleted", false)).toContain("last text");
  });
});
