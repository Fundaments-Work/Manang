import { describe, expect, it } from "vitest";

import { latexApi, projectApi, type EngineProgress } from "./api";
import { createDevKernel, DEMO_PROJECTS } from "./dev";
import { createMockKernel } from "./mock";
import { latexPlugin } from "./mockLatex";
import { MockProjectHost, projectPlugin } from "./mockProject";

const THESIS = "/home/author/thesis-draft";

function bare() {
  const host = new MockProjectHost(DEMO_PROJECTS, []);
  return createMockKernel({
    plugins: [projectPlugin(host), latexPlugin(host, { tex: false })],
  });
}

describe("mock built-in LaTeX engine", () => {
  it("offers the download, and never starts it without consent", async () => {
    const kernel = bare();
    const latex = latexApi(kernel);
    const status = await latex.engineStatus();
    expect(status).toMatchObject({ installed: false, downloadable: true });
    expect(status.size).toBeGreaterThan(0);
    expect(status.sha256).toMatch(/^[0-9a-f]{64}$/);

    await expect(latex.engineInstall(false)).rejects.toMatchObject({
      message: expect.stringContaining("[consent_required]"),
    });
    expect((await latex.engineStatus()).installed).toBe(false);
  });

  it("installs with progress, then compiles with the built-in engine", async () => {
    const kernel = bare();
    const latex = latexApi(kernel);
    await projectApi(kernel).open(THESIS);
    const main = `${THESIS}/main.tex`;
    await expect(latex.compile({ main })).rejects.toMatchObject({
      message: expect.stringContaining("[no_tex]"),
    });
    expect((await latex.status()).default).toBe("none");

    const seen: EngineProgress[] = [];
    const stop = await latex.onEngineProgress((p) => seen.push(p));
    const after = await latex.engineInstall(true);
    await new Promise((done) => setTimeout(done, 5));
    stop();
    expect(after.installed).toBe(true);
    expect(seen.some((p) => p.phase === "downloading")).toBe(true);
    expect(seen.at(-1)?.phase).toBe("done");

    expect((await latex.status()).default).toBe("tectonic");
    const report = await latex.compile({ main });
    expect(report).toMatchObject({ ok: true, backend: "tectonic", engine: "xelatex" });

    const gone = await latex.engineRemove();
    expect(gone.installed).toBe(false);
    expect((await latex.status()).default).toBe("none");
  });

  it("can be cancelled and then leaves nothing installed", async () => {
    const kernel = bare();
    const latex = latexApi(kernel);
    await kernel.run("dev.latex-engine-mode", { mode: "slow" });
    const running = latex.engineInstall(true);
    await new Promise((done) => setTimeout(done, 150));
    await latex.engineCancel();
    await expect(running).rejects.toMatchObject({
      message: expect.stringContaining("[engine_cancelled]"),
    });
    expect((await latex.engineStatus()).installed).toBe(false);
  });

  it("keeps system TeX first in automatic mode", async () => {
    const kernel = createDevKernel();
    const latex = latexApi(kernel);
    await latex.engineInstall(true);
    expect((await latex.status()).default).toBe("system");
  });
});
