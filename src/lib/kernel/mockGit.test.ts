import { describe, expect, it } from "vitest";
import { createDevKernel, DEMO_ROOT } from "./dev";
import { gitApi } from "./api";

describe("mockGit plugin", () => {
  it("opens repository and retrieves git.status", async () => {
    const kernel = createDevKernel();
    const git = gitApi(kernel);

    const open = await git.open(DEMO_ROOT);
    expect(open.is_repo).toBe(true);

    const status = await git.status({ root: DEMO_ROOT });
    expect(status.branch).toBe("main");
    expect(status.files.length).toBeGreaterThan(0);
  });

  it("stages, unstages, and commits files", async () => {
    const kernel = createDevKernel();
    const git = gitApi(kernel);

    // Initial status has modified files (e.g. sections/results.typ)
    const initialStatus = await git.status({ root: DEMO_ROOT });
    const modified = initialStatus.files.find((f) => f.unstaged === "modified");
    expect(modified).toBeDefined();

    if (modified) {
      // Stage the file
      await git.stage({ paths: [modified.path], root: DEMO_ROOT });
      const stagedStatus = await git.status({ root: DEMO_ROOT });
      expect(stagedStatus.files.find((f) => f.path === modified.path)?.staged).toBe("modified");

      // Commit
      const commit = await git.commit({ message: "feat: update results", root: DEMO_ROOT });
      expect(commit.short).toBeTruthy();
      expect(commit.subject).toBe("feat: update results");

      // Now it is committed and clean, so it should not appear in status
      const afterStatus = await git.status({ root: DEMO_ROOT });
      expect(afterStatus.files.find((f) => f.path === modified.path)).toBeUndefined();
    }
  });

  it("lists branches and checks out a new branch", async () => {
    const kernel = createDevKernel();
    const git = gitApi(kernel);

    const branches = await git.branches({ root: DEMO_ROOT });
    expect(branches.some((b) => b.name === "main")).toBe(true);

    const checkout = await git.checkoutBranch({ name: "test-branch", create: true, root: DEMO_ROOT });
    expect(checkout.branch).toBe("test-branch");
    expect(checkout.created).toBe(true);

    const status = await git.status({ root: DEMO_ROOT });
    expect(status.branch).toBe("test-branch");
  });

  it("fetches log and diff", async () => {
    const kernel = createDevKernel();
    const git = gitApi(kernel);

    const log = await git.log({ root: DEMO_ROOT });
    expect(log.commits.length).toBeGreaterThan(0);

    const diff = await git.diff({ root: DEMO_ROOT });
    expect(diff.files).toBeDefined();
  });
});
