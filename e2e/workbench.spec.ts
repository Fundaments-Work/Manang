/**
 * Playwright specification of the workbench, against the in-memory dev kernel
 * (`pnpm dev`). It mirrors `scripts/verify-ui.mjs`, which runs the same flows
 * headlessly with only Chromium and `ws`; add `@playwright/test` and a config
 * with `baseURL: "http://127.0.0.1:1420"` to run this one.
 *
 * Covers: welcome and recent projects, opening a project, activity-bar
 * switching, the file tree (mouse and keyboard), tabs (open, dirty, close,
 * shortcuts), compile feedback (preview, Problems, gutter), the command
 * palette, slides, the figure editor, zen mode and the narrow layouts.
 */

import { expect, test, type Page } from "@playwright/test";

async function openDemoProject(page: Page): Promise<void> {
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await page.getByRole("button", { name: /Entropy paper/ }).click();
  await expect(page.locator(".cm-content")).toContainText("Landauer");
}

test.describe("workbench", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 820 });
  });

  test("welcome lists recent projects and disables missing folders", async ({
    page,
  }) => {
    await page.goto("/");
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await expect(page.getByRole("button", { name: "Open folder" })).toBeVisible();
    await expect(page.getByRole("button", { name: "New project" })).toBeVisible();
    await expect(page.getByRole("button", { name: /Old thesis/ })).toBeDisabled();
  });

  test("opening a project shows files, a tab, a preview and compile timing", async ({
    page,
  }) => {
    await openDemoProject(page);
    await expect(page.getByRole("tab", { name: /main.typ/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(page.getByRole("img", { name: /Page 1/ })).toBeVisible();
    await expect(page.getByRole("contentinfo", { name: "Status" })).toContainText(
      /Compiled in \d+ ms/,
    );
  });

  test("activity bar switches panels and collapses on a second click", async ({
    page,
  }) => {
    await openDemoProject(page);
    for (const label of [
      "Search",
      "Figures",
      "References",
      "Plugins",
      "Settings",
      "Files",
    ]) {
      await page.getByRole("button", { name: label, exact: true }).click();
      await expect(
        page.getByRole("complementary", { name: `${label} panel` }),
      ).toBeVisible();
    }
    await page.getByRole("button", { name: "Files", exact: true }).click();
    await expect(page.locator("#side-panel")).toHaveCount(0);
  });

  test("the file tree answers the keyboard", async ({ page }) => {
    await openDemoProject(page);
    await page.getByRole("treeitem", { name: /sections/ }).focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.getByRole("treeitem", { name: /results.typ/ })).toBeVisible();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("tab", { name: /background.typ/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  test("typing an error reaches the status bar, Problems and the gutter", async ({
    page,
  }) => {
    await openDemoProject(page);
    await page.locator(".cm-content").click();
    await page.keyboard.press("Control+End");
    await page.keyboard.insertText("\nA broken formula $x + y\n");
    await expect(page.getByRole("contentinfo", { name: "Status" })).toContainText(
      "1 error",
    );
    await expect(page.locator(".manang-problem-error").first()).toBeVisible();
    await page
      .getByRole("contentinfo", { name: "Status" })
      .getByRole("button", { name: /error/ })
      .click();
    await expect(page.locator("#bottom-panel")).toContainText(
      "Unclosed math delimiter",
    );
  });

  test("tabs mark unsaved changes and ask before closing", async ({ page }) => {
    await openDemoProject(page);
    await page.locator(".cm-content").click();
    await page.keyboard.insertText("x");
    await expect(page.getByRole("tab", { name: /unsaved changes/ })).toBeVisible();
    await page.keyboard.press("Control+w");
    await expect(page.getByRole("alertdialog")).toContainText(
      "Save changes to main.typ",
    );
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+s");
    await expect(page.getByRole("tab", { name: /unsaved changes/ })).toHaveCount(0);
  });

  test("palette opens with Ctrl+K, filters, and runs a workbench command", async ({
    page,
  }) => {
    await openDemoProject(page);
    await page.keyboard.press("Control+k");
    const box = page.getByRole("combobox");
    await expect(box).toBeFocused();
    await box.fill("problems");
    await page.keyboard.press("Enter");
    await expect(page.locator("#bottom-panel")).toBeVisible();
  });

  test("slides view and presentation live inside the tab", async ({ page }) => {
    await openDemoProject(page);
    await page.keyboard.press("Control+p");
    await page.getByRole("combobox").fill("talk");
    await page.keyboard.press("Enter");
    await page.getByRole("radio", { name: "Slides" }).click();
    await expect(
      page.getByRole("listbox", { name: "Slides" }).getByRole("option"),
    ).toHaveCount(6);
    await page.getByRole("button", { name: /Present/ }).click();
    await expect(page.getByRole("dialog", { name: "Presentation" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog", { name: "Presentation" })).toHaveCount(0);
  });

  test("a figure opens in a tab and drawing marks it dirty", async ({ page }) => {
    await openDemoProject(page);
    await page.getByRole("button", { name: "Figures", exact: true }).click();
    await page.getByRole("button", { name: /state_lattice/ }).click();
    const canvas = page.getByRole("application");
    await expect(canvas).toBeVisible();
    await page.getByRole("button", { name: "Rectangle" }).click();
    const box = await canvas.boundingBox();
    if (box === null) throw new Error("no canvas");
    await page.mouse.move(box.x + 200, box.y + 380);
    await page.mouse.down();
    await page.mouse.move(box.x + 320, box.y + 450);
    await page.mouse.up();
    await expect(page.getByRole("tab", { name: /unsaved changes/ })).toBeVisible();
  });

  test("zen removes all chrome and keeps a 680px column", async ({ page }) => {
    await openDemoProject(page);
    await page.keyboard.press("Control+\\");
    await expect(page.locator("header, footer, nav, aside")).toHaveCount(0);
    const width = await page
      .locator(".zen-editor .cm-content")
      .evaluate((el) => el.getBoundingClientRect().width);
    expect(width).toBeLessThanOrEqual(680);
    await page.keyboard.press("Escape");
    await expect(page.locator("header")).toBeVisible();
  });

  test("under 800px the side panel overlays and split falls back to the editor", async ({
    page,
  }) => {
    await openDemoProject(page);
    await page.setViewportSize({ width: 760, height: 700 });
    await expect(
      page.getByRole("navigation", { name: "Workbench views" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Files", exact: true }).click();
    const position = await page
      .locator("#side-panel")
      .evaluate((el) => getComputedStyle(el).position);
    expect(position).toBe("absolute");
    await page.keyboard.press("Escape");
    await expect(page.getByRole("region", { name: "Preview" })).toHaveCount(0);
    await page.setViewportSize({ width: 640, height: 600 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    );
    expect(overflow).toBe(true);
  });
});
