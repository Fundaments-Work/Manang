import { describe, expect, it, vi } from "vitest";

import { SurfaceRegistry, type FocusSurface } from "./surface";

/** Fake elements: a surface "contains" the elements in its set. */
function surface(
  id: string,
  members: object[],
  actions: FocusSurface["actions"],
  extra: Partial<FocusSurface> = {},
): FocusSurface {
  return { id, contains: (el) => members.includes(el), actions, ...extra };
}

describe("the focused-surface dispatcher", () => {
  const editorEl = {};
  const inputEl = {};
  const canvasEl = {};

  function setup() {
    const registry = new SurfaceRegistry();
    const editor = { undo: vi.fn(), find: vi.fn(), copy: vi.fn() };
    const native = { undo: vi.fn(), copy: vi.fn() };
    const canvas = { undo: vi.fn() };
    registry.register(surface("editor", [editorEl], editor));
    // The native fallback "contains" every editable element, the editor's too.
    registry.register(
      surface("native", [editorEl, inputEl], native, { fallback: true }),
    );
    const stopCanvas = registry.register(surface("figure", [canvasEl], canvas));
    return { registry, editor, native, canvas, stopCanvas };
  }

  it("sends an action to the surface that holds focus", async () => {
    const { registry, editor, native, canvas } = setup();
    expect(await registry.dispatch("undo", editorEl)).toBe(true);
    expect(editor.undo).toHaveBeenCalledTimes(1);
    expect(native.undo).not.toHaveBeenCalled();
    await registry.dispatch("undo", canvasEl);
    expect(canvas.undo).toHaveBeenCalledTimes(1);
    await registry.dispatch("copy", inputEl);
    expect(native.copy).toHaveBeenCalledTimes(1);
  });

  it("does not hand an action to another surface when the focused one cannot do it", async () => {
    const { registry, editor } = setup();
    registry.noteFocus(editorEl);
    expect(registry.can("find", canvasEl)).toBe(false);
    expect(await registry.dispatch("find", canvasEl)).toBe(false);
    expect(editor.find).not.toHaveBeenCalled();
  });

  it("falls back to the surface focused last when focus is elsewhere (a menu)", async () => {
    const { registry, editor } = setup();
    const menuButton = {};
    expect(registry.can("find", menuButton)).toBe(false);
    registry.noteFocus(editorEl);
    registry.noteFocus(menuButton); // not a surface: remembered one stays
    expect(registry.can("find", menuButton)).toBe(true);
    await registry.dispatch("find", null);
    expect(editor.find).toHaveBeenCalledTimes(1);
  });

  it("never targets a native field that no longer has focus", () => {
    const { registry } = setup();
    registry.noteFocus(inputEl);
    expect(registry.can("copy", {})).toBe(false);
  });

  it("forgets surfaces that unregister or are gone", () => {
    const { registry, stopCanvas } = setup();
    registry.noteFocus(canvasEl);
    expect(registry.can("undo", null)).toBe(true);
    stopCanvas();
    expect(registry.can("undo", canvasEl)).toBe(false);
    expect(registry.can("undo", null)).toBe(false);
    let alive = true;
    registry.register(
      surface("pdf", [canvasEl], { copy: vi.fn() }, { available: () => alive }),
    );
    expect(registry.can("copy", canvasEl)).toBe(true);
    alive = false;
    expect(registry.can("copy", canvasEl)).toBe(false);
  });

  it("registering an id again replaces the old surface", async () => {
    const registry = new SurfaceRegistry();
    const first = vi.fn();
    const second = vi.fn();
    registry.register(surface("editor", [editorEl], { undo: first }));
    registry.register(surface("editor", [editorEl], { undo: second }));
    await registry.dispatch("undo", editorEl);
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });
});
