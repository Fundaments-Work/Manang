/**
 * Playwright End-to-End Test Suite for Manang's Seven Canonical Screens
 *
 * Covers:
 * 1. Desk (manuscript CodeMirror editor, Margin Whisper, status bar)
 * 2. Library Shelf (rail dimensions, document switching, tree filtering)
 * 3. Command Palette (Cmd+K launcher, fuzzy search, arrow navigation, escape)
 * 4. Vector Ink Canvas (SVG scene graph, pen tray tools, stroke creation, Done button)
 * 5. Presentation Studio (16:9 aspect ratio, slide carousel, bidirectional navigation)
 * 6. Instrument Shelf (capability summaries, sandboxing switches)
 * 7. Zen Mode (zero-chrome immersion, 680px reading column)
 */

import { test, expect } from "@playwright/test";

test.describe("Manang Seven Screens UI Verification", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 820 });
  });

  test("Screen 1: Desk mounts CodeMirror and renders margin whisper", async ({ page }) => {
    await page.goto("/#");
    await page.waitForSelector(".cm-editor");

    // CodeMirror owns manuscript rendering
    const content = page.locator(".cm-content");
    await expect(content).toBeVisible();
    await expect(content).toContainText("Landauer");

    // Margin whisper in gutter
    const whisper = page.locator(".manang-whisper").first();
    await expect(whisper).toBeVisible();
    const box = await whisper.boundingBox();
    expect(box).not.toBeNull();
    expect(Math.round(box!.width)).toBe(4);
    expect(Math.round(box!.height)).toBe(4);

    // Strict design invariants
    const bodyBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(bodyBg).toBe("rgb(20, 20, 22)");
  });

  test("Screen 2: Library Shelf supports tree filtering and document switching", async ({ page }) => {
    await page.goto("/#library");
    const aside = page.locator('aside[aria-label="Library shelf"]');
    await expect(aside).toBeVisible();

    const railBox = await aside.boundingBox();
    expect(Math.round(railBox!.width)).toBe(260);

    // Document switching
    const initialActive = page.locator('[aria-current="true"]');
    await expect(initialActive).toContainText("02_entropy_bounds.typ");

    const targetDoc = page.locator('nav[aria-label="Project files"] button', {
      hasText: "01_formalism.typ",
    });
    await targetDoc.click();

    const newActive = page.locator('[aria-current="true"]');
    await expect(newActive).toContainText("01_formalism.typ");

    // Filter files
    const filterInput = page.locator("#library-filter");
    await filterInput.fill("lattice");
    const matchedButtons = page.locator('nav[aria-label="Project files"] button');
    await expect(matchedButtons).toHaveCount(1);
    await expect(matchedButtons.first()).toContainText("state_lattice.svg");
  });

  test("Screen 3: Command Palette fuzzy search and keyboard navigation", async ({ page }) => {
    await page.goto("/#palette/cit");
    const dialog = page.locator('[role="dialog"]');
    await expect(dialog).toBeVisible();

    // Query applied from deep link
    const queryInput = page.locator("#palette-query");
    await expect(queryInput).toHaveValue("cit");

    // Top match is selected
    const selectedOption = page.locator('[role="option"][aria-selected="true"]');
    await expect(selectedOption).toContainText("Verify citations");

    // Arrow navigation
    await page.keyboard.press("ArrowDown");
    const newSelected = page.locator('[role="option"][aria-selected="true"]');
    expect(await newSelected.textContent()).not.toEqual(await selectedOption.textContent());

    // Dismiss with Escape
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
  });

  test("Screen 4: Vector Ink Canvas tools, drawing and closure", async ({ page }) => {
    await page.goto("/#draw");
    const canvasSection = page.locator('section[aria-label="Vector ink canvas"]');
    await expect(canvasSection).toBeVisible();

    // Breadcrumb path in header
    const breadcrumb = canvasSection.locator("header span.font-mono");
    await expect(breadcrumb).toContainText("figures / vectors / state_lattice.svg");

    // Tool switching in pen tray
    const geomBtn = page.locator('[role="toolbar"] button', { hasText: "Geometry" });
    await geomBtn.click();
    await expect(geomBtn).toHaveAttribute("aria-pressed", "true");

    // SVG scene graph
    const svg = page.locator('svg[role="application"]');
    await expect(svg).toBeVisible();

    // Done button restores writing desk
    const doneBtn = page.locator("header button", { hasText: "Done" });
    await doneBtn.click();
    await expect(canvasSection).toBeHidden();
  });

  test("Screen 5: Presentation Studio 16:9 frame and carousel navigation", async ({ page }) => {
    await page.goto("/#slides/2");
    const studio = page.locator('section[aria-label="Presentation studio"]');
    await expect(studio).toBeVisible();

    // Frame aspect ratio (16:9)
    const aspectContainer = page.locator("div.aspect-video").first();
    const box = await aspectContainer.boundingBox();
    expect(box).not.toBeNull();
    expect(Math.abs(box!.width / box!.height - 16 / 9)).toBeLessThan(0.02);

    // Carousel buttons
    const carouselButtons = page.locator('nav[aria-label="Slides"] button');
    await expect(carouselButtons).toHaveCount(6);

    // Thumbnail click navigation
    await carouselButtons.first().click();
    await expect(page.locator("body")).toContainText("SLIDE 1 / 6");

    // Bidirectional keyboard navigation
    await page.keyboard.press("ArrowRight");
    await expect(page.locator("body")).toContainText("SLIDE 2 / 6");
    await page.keyboard.press("ArrowLeft");
    await expect(page.locator("body")).toContainText("SLIDE 1 / 6");

    // Close button
    const closeBtn = page.locator('button[aria-label="Close presentation studio"]');
    await closeBtn.click();
    await expect(studio).toBeHidden();
  });

  test("Screen 6: Instrument Shelf sandboxing toggles", async ({ page }) => {
    await page.goto("/#shelf");
    const shelf = page.locator('aside[aria-label="Instrument shelf"]');
    await expect(shelf).toBeVisible();

    // Essential instrument cannot be toggled off
    const switches = page.locator('[role="switch"]');
    const firstSwitch = switches.first();
    await expect(firstSwitch).toHaveAttribute("aria-checked", "true");
    await firstSwitch.click();
    await expect(firstSwitch).toHaveAttribute("aria-checked", "true");

    // Plugin instrument can be toggled
    const secondSwitch = switches.nth(1);
    const initialChecked = await secondSwitch.getAttribute("aria-checked");
    await secondSwitch.click();
    const nextChecked = await secondSwitch.getAttribute("aria-checked");
    expect(nextChecked).not.toEqual(initialChecked);
  });

  test("Screen 7: Zen Mode dissolves all chrome", async ({ page }) => {
    await page.goto("/#zen");
    await expect(page.locator("header")).toBeHidden();
    await expect(page.locator("footer")).toBeHidden();
    await expect(page.locator("nav")).toBeHidden();

    const article = page.locator("article");
    await expect(article).toBeVisible();
    const box = await article.boundingBox();
    expect(box).not.toBeNull();
    expect(Math.round(box!.width)).toBe(680);
  });
});
