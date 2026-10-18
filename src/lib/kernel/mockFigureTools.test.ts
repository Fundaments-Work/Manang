import { describe, expect, it } from "vitest";

import { projectApi } from "./api";
import { DEMO_ROOT, createDevKernel } from "./dev";
import { looksLikeMermaid } from "./mockFigureTools";

/** The browser mock's agent tools, driven the way an agent drives the Rust ones. */
async function setup() {
  const kernel = createDevKernel();
  await projectApi(kernel).open(DEMO_ROOT);
  const path = `${DEMO_ROOT}/figures/agent.figure.json`;
  await kernel.run("figure.create", { path, width: 960, height: 540 });
  const events: unknown[] = [];
  await kernel.onEvent((e) => {
    if (e.topic === "figure.changed") events.push(e.payload);
  });
  const call = <T = Record<string, unknown>>(
    command: string,
    args: Record<string, unknown>,
  ) => kernel.run<T>(command, { path, ...args });
  return { kernel, path, call, events };
}

describe("browser figure agent tools", () => {
  it("builds, connects, lays out, describes and exports a flowchart", async () => {
    const { call, events } = await setup();
    const added = await call<{ ids: string[] }>("figure.add-shapes", {
      shapes: [
        { kind: "ellipse", label: "Start" },
        { label: "Load data" },
        { kind: "diamond", label: "Valid?" },
        { label: "Fit model" },
      ],
    });
    expect(added.ids).toEqual(["n1", "n2", "n3", "n4"]);
    const connected = await call<{ ids: string[] }>("figure.connect", {
      connections: [
        { from: "Start", to: "load data" },
        { from: "Load Data", to: "Valid?" },
        { from: "Valid?", to: "Fit model", label: "yes" },
      ],
    });
    expect(connected.ids).toHaveLength(3);
    await call("figure.layout", { direction: "TB" });
    const d = await call<{ shapes: { id: string; y: number }[]; summary: string }>(
      "figure.describe",
      {},
    );
    const y = (id: string) => d.shapes.find((s) => s.id === id)?.y ?? Number.NaN;
    expect(y("n1")).toBeLessThan(y("n2"));
    expect(y("n2")).toBeLessThan(y("n3"));
    expect(d.summary).toContain('"Valid?" (n3) -> "Fit model" (n4) labelled "yes"');
    const out = await call<{ svg: string }>("figure.export", {});
    expect(out.svg).toMatch(/agent\.svg$/);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(events.length).toBe(3);
    const mermaid = await call<{ source: string }>("figure.to-mermaid", {});
    expect(mermaid.source).toContain('n3 -->|"yes"| n4');
  });

  it("turns Mermaid into bound shapes and falls back to a block", async () => {
    const { call } = await setup();
    const flow = await call<{ ids: string[]; mode: string }>("figure.from-mermaid", {
      source: "flowchart TD\n  A[Start] --> B{Ok?}\n  B -->|yes| C[Done]\n",
      x: 40,
      y: 40,
    });
    expect(flow.mode).toBe("shapes");
    const d = await call<{
      shapes: unknown[];
      connections: { from: string; to: string }[];
    }>("figure.describe", {});
    expect(d.shapes).toHaveLength(3);
    expect(d.connections.every((c) => c.from !== undefined && c.to !== undefined)).toBe(
      true,
    );
    const pie = await call<{ mode: string; fallback: string }>("figure.from-mermaid", {
      source: 'pie\n  "a" : 1\n',
    });
    expect(pie.mode).toBe("block");
    expect(pie.fallback).toContain("unsupported_layout");
  });

  it("answers mistakes with the plugin's tags and hints", async () => {
    const { call } = await setup();
    await call("figure.add-shapes", {
      shapes: [{ label: "Start" }, { label: "Start" }, { label: "End" }],
    });
    await expect(
      call("figure.connect", { connections: [{ from: "Strat", to: "End" }] }),
    ).rejects.toMatchObject({
      message: expect.stringContaining('did you mean "Start"'),
    });
    await expect(
      call("figure.connect", { connections: [{ from: "Start", to: "End" }] }),
    ).rejects.toMatchObject({ message: expect.stringContaining("[ambiguous]") });
    await expect(
      call("figure.style", { refs: ["End"], style: { stroke: "red" } }),
    ).rejects.toMatchObject({ message: expect.stringContaining("vermillion") });
  });

  it("lists the figure tools from the same declarations the Rust plugin uses", async () => {
    const { kernel } = await setup();
    const list = await kernel.agentTools?.();
    expect(list?.problems).toEqual([]);
    expect(list?.tools.map((t) => t.name)).toContain("figure_add_shapes");
    expect(
      list?.tools.every(
        (t) => t.plugin === "figure" && t.command.startsWith("figure."),
      ),
    ).toBe(true);
    expect(list?.tools).toHaveLength(10);
  });

  it("recognises Mermaid text", () => {
    expect(looksLikeMermaid("\n%% c\nflowchart LR\n A-->B")).toBe(true);
    expect(looksLikeMermaid("sequenceDiagram\n A->>B: hi")).toBe(true);
    expect(looksLikeMermaid("just some words")).toBe(false);
  });
});
