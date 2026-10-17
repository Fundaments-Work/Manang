import { describe, expect, it } from "vitest";
import { createDevKernel, DEMO_ROOT } from "./dev";
import { searchApi } from "./api";

describe("mockSearch plugin", () => {
  it("searches file names with search.files", async () => {
    const kernel = createDevKernel();
    const search = searchApi(kernel);
    const res = await search.files({ root: DEMO_ROOT, query: "state" });
    expect(res.results.length).toBeGreaterThan(0);
    expect(res.results.some((r) => r.path.includes("state_lattice"))).toBe(true);
  });

  it("searches text with search.text (case, word, regex)", async () => {
    const kernel = createDevKernel();
    const search = searchApi(kernel);

    // Literal search
    const res1 = await search.text({ root: DEMO_ROOT, query: "entropy" });
    expect(res1.total_matches).toBeGreaterThan(0);
    expect(res1.files.some((f) => f.path === "main.typ")).toBe(true);

    // Whole word search
    const resWord = await search.text({
      root: DEMO_ROOT,
      query: "entropy",
      whole_word: true,
    });
    expect(resWord.total_matches).toBeGreaterThan(0);

    // Regex search
    const resRegex = await search.text({
      root: DEMO_ROOT,
      query: "k_B\\s+T",
      regex: true,
    });
    expect(resRegex.total_matches).toBeGreaterThan(0);
  });

  it("previews and applies replace across files", async () => {
    const kernel = createDevKernel();
    const search = searchApi(kernel);

    const preview = await search.replacePreview({
      root: DEMO_ROOT,
      query: "irreversible",
      replacement: "non-reversible",
    });

    expect(preview.total_edits).toBeGreaterThan(0);
    expect(preview.plan_id).toBeTruthy();
    expect(preview.files.length).toBeGreaterThan(0);

    const apply = await search.replaceApply({ plan_id: preview.plan_id });
    expect(apply.applied).toBeGreaterThan(0);
    expect(apply.failed).toBe(0);

    // Verify content actually updated
    const after = await search.text({ root: DEMO_ROOT, query: "non-reversible" });
    expect(after.total_matches).toBeGreaterThan(0);
  });

  it("handles search.cancel", async () => {
    const kernel = createDevKernel();
    const search = searchApi(kernel);
    const res = await search.cancel();
    expect(res.cancelled).toBe(1);
  });
});
