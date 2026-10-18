import { describe, expect, it, vi } from "vitest";
import { createDevKernel, DEMO_ROOT } from "./dev";
import { gitApi, projectApi } from "./api";
import { MOCK_IDENTITY_FLAG } from "./mockGit";

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

  describe("versions", () => {
    async function setup() {
      const kernel = createDevKernel();
      const git = gitApi(kernel);
      const project = projectApi(kernel);
      await project.open(DEMO_ROOT);
      await git.open(DEMO_ROOT);
      return { git, project };
    }

    it("saves a labelled version at the top of the timeline", async () => {
      const { git, project } = await setup();
      await project.writeFile("notes.md", "# A note\n");
      const saved = await git.saveVersion({ root: DEMO_ROOT, name: "First draft" });
      expect(saved.subject).toBe("First draft");
      expect(saved.files_changed).toBeGreaterThan(0);
      const page = await git.versions({ root: DEMO_ROOT });
      expect(page.versions[0]).toMatchObject({ subject: "First draft", labelled: true });
      expect(page.versions[1]?.labelled).toBe(false);
      await expect(git.saveVersion({ root: DEMO_ROOT })).rejects.toMatchObject({
        message: expect.stringContaining("nothing-to-save"),
      });
    });

    it("restores a file, refusing a dirty one unless a safety version is asked for", async () => {
      const { git, project } = await setup();
      await project.writeFile("notes.md", "one\n");
      const first = await git.saveVersion({ root: DEMO_ROOT, name: "One" });
      await project.writeFile("notes.md", "two\n");
      await git.saveVersion({ root: DEMO_ROOT, name: "Two" });
      await project.writeFile("notes.md", "unsaved\n");
      await expect(
        git.restore({ root: DEMO_ROOT, rev: first.sha, paths: ["notes.md"] }),
      ).rejects.toMatchObject({ message: expect.stringContaining('"paths":["notes.md"]') });
      expect(await project.readFile("notes.md")).toBe("unsaved\n");
      const done = await git.restore({
        root: DEMO_ROOT,
        rev: first.sha,
        paths: ["notes.md"],
        save_first: true,
      });
      expect(done.restored).toEqual(["notes.md"]);
      expect(done.safety_version).not.toBeNull();
      expect(await project.readFile("notes.md")).toBe("one\n");
      const page = await git.versions({ root: DEMO_ROOT });
      expect(page.versions[0]?.subject).toBe(`Before restoring ${first.short}`);
    });

    it("lists and diffs what a version changed", async () => {
      const { git, project } = await setup();
      await project.writeFile("notes.md", "changed\n");
      const saved = await git.saveVersion({ root: DEMO_ROOT, name: "Change" });
      const files = await git.versionFiles({ root: DEMO_ROOT, rev: saved.sha });
      expect(files.files.map((f) => f.path)).toContain("notes.md");
      const diff = await git.versionDiff({ root: DEMO_ROOT, rev: saved.sha, path: "notes.md" });
      expect(diff.additions).toBeGreaterThan(0);
      await expect(git.versionFiles({ root: DEMO_ROOT, rev: "--exec=x" })).rejects.toBeDefined();
    });

    it("asks for an identity when the dev flag says there is none", async () => {
      const stored = new Map<string, string>([[MOCK_IDENTITY_FLAG, "none"]]);
      vi.stubGlobal("localStorage", {
        getItem: (k: string) => stored.get(k) ?? null,
      });
      try {
        const { git, project } = await setup();
        expect((await git.identityGet({ root: DEMO_ROOT })).source).toBe("none");
        await project.writeFile("notes.md", "x\n");
        await expect(git.saveVersion({ root: DEMO_ROOT })).rejects.toMatchObject({
          message: expect.stringContaining("missing-identity"),
        });
        await expect(
          git.identitySet({ root: DEMO_ROOT, name: "Ada", email: "nope" }),
        ).rejects.toBeDefined();
        await git.identitySet({ root: DEMO_ROOT, name: "Ada", email: "ada@example.org" });
        expect(await git.identityGet({ root: DEMO_ROOT })).toMatchObject({
          name: "Ada",
          source: "repo",
        });
        const saved = await git.saveVersion({ root: DEMO_ROOT, name: "v" });
        expect(saved.subject).toBe("v");
        expect((await git.versions({ root: DEMO_ROOT })).versions[0]?.author_name).toBe("Ada");
      } finally {
        vi.unstubAllGlobals();
      }
    });
  });
});
