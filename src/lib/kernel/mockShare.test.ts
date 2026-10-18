import { afterEach, describe, expect, it, vi } from "vitest";

import { gitApi } from "./api";
import { createDevKernel, DEMO_ROOT } from "./dev";
import { MOCK_SHARE_FLAG } from "./mockGit";
import { parseGitError } from "$lib/workbench/git";

const stored = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (k: string) => stored.get(k) ?? null,
});

function flag(value: string | null): void {
  if (value === null) stored.delete(MOCK_SHARE_FLAG);
  else stored.set(MOCK_SHARE_FLAG, value);
}

async function code(promise: Promise<unknown>): Promise<string> {
  try {
    await promise;
  } catch (error) {
    return parseGitError(error).code;
  }
  return "none";
}

describe("mock share and get updates", () => {
  afterEach(() => flag(null));

  it("starts with no shared location, then follows connect and share", async () => {
    const git = gitApi(createDevKernel());
    await git.open(DEMO_ROOT);
    expect((await git.syncStatus({ root: DEMO_ROOT })).state).toBe("no-remote");
    expect(await code(git.share({ root: DEMO_ROOT }))).toBe("no-remote");
    expect(await code(git.remoteSet({ root: DEMO_ROOT, url: "a b" }))).toBe(
      "bad-remote-url",
    );
    const set = await git.remoteSet({ root: DEMO_ROOT, url: "https://x.org/p.git" });
    expect(set.url).toBe("https://x.org/p.git");
    const before = await git.syncStatus({ root: DEMO_ROOT });
    expect(before).toMatchObject({ state: "ahead", never_shared: true });
    const shared = await git.share({ root: DEMO_ROOT });
    expect(shared.pushed).toBe(before.ahead);
    expect((await git.syncStatus({ root: DEMO_ROOT })).state).toBe("up-to-date");
  });

  it("gets updates when behind and then is up to date", async () => {
    const git = gitApi(createDevKernel());
    await git.open(DEMO_ROOT);
    await git.remoteSet({ root: DEMO_ROOT, url: "https://x.org/p.git" });
    await git.share({ root: DEMO_ROOT });
    flag("behind");
    expect((await git.syncStatus({ root: DEMO_ROOT })).state).toBe("behind");
    expect(await code(git.share({ root: DEMO_ROOT }))).toBe("share-rejected");
    const got = await git.getUpdates({ root: DEMO_ROOT });
    expect(got.result).toBe("merged");
    expect(got.safety_version).not.toBeNull();
    expect((await git.getUpdates({ root: DEMO_ROOT })).result).toBe("up-to-date");
  });

  it("reports diverged and conflicts without changing anything", async () => {
    const git = gitApi(createDevKernel());
    await git.open(DEMO_ROOT);
    await git.remoteSet({ root: DEMO_ROOT, url: "https://x.org/p.git" });
    flag("diverged");
    expect((await git.syncStatus({ root: DEMO_ROOT })).state).toBe("diverged");
    const ff = await git.getUpdates({ root: DEMO_ROOT, strategy: "ff-only" });
    expect(ff.result).toBe("diverged");
    flag("conflicts");
    const clash = await git.getUpdates({ root: DEMO_ROOT });
    expect(clash.result).toBe("conflicts");
    expect(clash.files.length).toBe(1);
    expect((await git.syncStatus({ root: DEMO_ROOT })).state).toBe("diverged");
  });

  it("fails readably when the host cannot be reached or refuses", async () => {
    const git = gitApi(createDevKernel());
    await git.open(DEMO_ROOT);
    await git.remoteSet({ root: DEMO_ROOT, url: "https://x.org/p.git" });
    flag("unreachable");
    expect(await code(git.share({ root: DEMO_ROOT }))).toBe("remote-unreachable");
    flag("auth");
    expect(await code(git.getUpdates({ root: DEMO_ROOT }))).toBe("auth-failed");
    flag("rejected");
    expect(await code(git.share({ root: DEMO_ROOT }))).toBe("share-rejected");
  });
});
