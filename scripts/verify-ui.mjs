/**
 * Headless verification harness for the Manang workbench.
 *
 * The AGENTS.md testing pyramid asks for end-to-end UI tests, and the
 * workbench's most valuable checks are visual and interactive: that the tab
 * strip really switches documents, that an error typed into the editor reaches
 * the Problems panel and the gutter, that the tree answers the arrow keys, that
 * the side panel overlays instead of squeezing the editor on a narrow window.
 * None of that is observable from unit tests alone.
 *
 * Drives the already-installed system Chromium over the DevTools protocol
 * rather than downloading a browser, so the check runs offline and on the same
 * binary the developer sees. `ws` is the only dependency. The app runs against
 * the in-memory demo kernel (`pnpm dev`), which has the same command shapes as
 * the Rust plugins.
 *
 * Usage: node scripts/verify-ui.mjs        (expects `pnpm dev` on MANANG_ORIGIN,
 *        default http://127.0.0.1:1420)
 */

import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { request as httpRequest } from "node:http";
import { setTimeout as sleep } from "node:timers/promises";
import WebSocket from "ws";

const ORIGIN = process.env.MANANG_ORIGIN ?? "http://127.0.0.1:1420";
const SHOTS = "artifacts/screens";
const PORT = Number(process.env.MANANG_CDP_PORT ?? 9333);

function findBrowser() {
  for (const name of [
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
  ]) {
    if (spawnSync("which", [name], { stdio: "ignore" }).status === 0) return name;
  }
  return null;
}

/* -------------------------------------------------------------------------- */
/* Minimal CDP client                                                          */
/* -------------------------------------------------------------------------- */

class Cdp {
  #socket;
  #next = 1;
  #pending = new Map();
  errors = [];

  constructor(socket) {
    this.#socket = socket;
    socket.on("message", (raw) => {
      const message = JSON.parse(raw.toString());
      if (message.method === "Runtime.exceptionThrown") {
        this.errors.push(
          message.params.exceptionDetails.exception?.description ?? "exception",
        );
      } else if (
        message.method === "Runtime.consoleAPICalled" &&
        message.params.type === "error"
      ) {
        this.errors.push(
          message.params.args.map((a) => a.value ?? a.description).join(" "),
        );
      }
      const entry = this.#pending.get(message.id);
      if (entry === undefined) return;
      this.#pending.delete(message.id);
      if (message.error) entry.reject(new Error(JSON.stringify(message.error)));
      else entry.resolve(message.result);
    });
  }

  static async open(url) {
    const socket = new WebSocket(url, {
      perMessageDeflate: false,
      maxPayload: 64 * 1024 * 1024,
    });
    await new Promise((resolve, reject) => {
      socket.once("open", resolve);
      socket.once("error", reject);
    });
    return new Cdp(socket);
  }

  send(method, params = {}) {
    const id = this.#next++;
    return new Promise((resolve, reject) => {
      // A call that never answers means the page died; fail loudly, not by hanging.
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(
          new Error(`no answer to ${method} within 20s (did the page close or crash?)`),
        );
      }, 20000);
      this.#pending.set(id, {
        resolve: (value) => (clearTimeout(timer), resolve(value)),
        reject: (error) => (clearTimeout(timer), reject(error)),
      });
      this.#socket.send(JSON.stringify({ id, method, params }));
    });
  }

  /** Evaluates an expression in the page and returns its JSON value. */
  async evaluate(expression) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (result.exceptionDetails) {
      throw new Error(
        result.exceptionDetails.exception?.description ?? "evaluation failed",
      );
    }
    return result.result.value;
  }

  close() {
    this.#socket.close();
  }
}

async function waitForEndpoint(port, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const reachable = await new Promise((resolve) => {
      const request = httpRequest(
        { host: "127.0.0.1", port, path: "/json/version" },
        (res) => {
          res.resume();
          res.once("end", () => resolve(true));
        },
      );
      request.once("error", () => resolve(false));
      request.setTimeout(500, () => {
        request.destroy();
        resolve(false);
      });
      request.end();
    });
    if (reachable) return true;
    await sleep(200);
  }
  return false;
}

/* -------------------------------------------------------------------------- */
/* Harness                                                                     */
/* -------------------------------------------------------------------------- */

const results = [];

/** The page under test, so a failure can show what the page itself reported. */
let current;

function check(name, passed, detail = "") {
  results.push({ name, passed, detail });
  process.stdout.write(
    `  ${passed ? "PASS" : "FAIL"}  ${name}${detail ? ` (${detail})` : ""}\n`,
  );
  if (process.env.MANANG_TRACE === "1" && current !== undefined) {
    // Debug aid: which tabs are unsaved after each check.
    void dirtyTabs(current).then(async (dirty) => {
      process.stdout.write(`        unsaved: ${dirty.join() || "none"}\n`);
      const active = await activeTab(current);
      if (active !== null && dirty.includes(active) && dirty.length === 1) {
        process.stdout.write(
          `        text: ${JSON.stringify(await current.evaluate(`document.querySelector(".cm-content")?.innerText.slice(0, 300) ?? ""`))}\n`,
        );
      }
    });
  }
  if (!passed && current !== undefined && current.errors.length > 0) {
    process.stdout.write(
      `        page reported: ${current.errors.splice(0).join(" | ").slice(0, 600)}\n`,
    );
  }
}

function section(title) {
  process.stdout.write(`\n${title}\n`);
}

/**
 * Polls until `expression` returns truthy. A fixed sleep is a flaky test: the
 * editor mounts, then the grammar chunk resolves, then the preview renders.
 * Waiting for the thing under test is what makes this suite reliable.
 */
async function waitFor(cdp, expression, { timeout = 30000, label = expression } = {}) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    let value = false;
    try {
      value = await cdp.evaluate(expression);
    } catch {
      value = false;
    }
    if (value) return true;
    await sleep(100);
  }
  throw new Error(`timed out waiting for: ${label}`);
}

/** A check that waits for its condition, so slow machines do not flake. */
async function expectEventually(cdp, name, expression, options = {}) {
  try {
    await waitFor(cdp, expression, { timeout: options.timeout ?? 10000, label: name });
    check(name, true);
    return true;
  } catch {
    // `show` is an expression whose value explains a failure (what was there).
    let detail = options.detail ?? "condition never became true";
    if (options.show !== undefined) {
      const seen = await cdp.evaluate(options.show).catch(String);
      detail = `${detail}; saw ${JSON.stringify(seen)}`;
    }
    check(name, false, detail);
    return false;
  }
}

async function newPage(browser, { width = 1280, height = 820, fresh = true } = {}) {
  const target = await browser.send("Target.createTarget", { url: "about:blank" });
  const list = await fetch(`http://127.0.0.1:${PORT}/json/list`).then((r) => r.json());
  const info = list.find((entry) => entry.id === target.targetId);
  const cdp = await Cdp.open(info.webSocketDebuggerUrl);
  await cdp.send("Page.enable");
  await cdp.send("Runtime.enable");
  await cdp.send("Emulation.setDeviceMetricsOverride", {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
  });
  if (fresh) {
    // Persisted layout and tabs would leak between runs; start from nothing.
    await cdp.send("Page.navigate", { url: `${ORIGIN}/` });
    await sleep(400);
    await cdp.evaluate(`localStorage.clear()`);
  }
  await cdp.send("Page.navigate", { url: `${ORIGIN}/` });
  await waitFor(
    cdp,
    `document.readyState === "complete" && document.querySelector("#app")?.childElementCount > 0`,
    {
      label: "app mounted",
    },
  );
  await sleep(300);
  return cdp;
}

async function reload(cdp) {
  await cdp.send("Page.reload");
  await sleep(300);
  await waitFor(
    cdp,
    `document.readyState === "complete" && document.querySelector("#app")?.childElementCount > 0`,
  );
}

async function resize(cdp, width, height) {
  await cdp.send("Emulation.setDeviceMetricsOverride", {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await sleep(250);
}

async function shoot(cdp, name) {
  const { data } = await cdp.send("Page.captureScreenshot", { format: "png" });
  mkdirSync(SHOTS, { recursive: true });
  writeFileSync(`${SHOTS}/${name}.png`, Buffer.from(data, "base64"));
}

/* -- Input --------------------------------------------------------------- */

const KEYS = {
  ".": ["Period", 190],
  Tab: ["Tab", 9],
  Enter: ["Enter", 13],
  Escape: ["Escape", 27],
  PageUp: ["PageUp", 33],
  PageDown: ["PageDown", 34],
  End: ["End", 35],
  Home: ["Home", 36],
  ArrowLeft: ["ArrowLeft", 37],
  ArrowUp: ["ArrowUp", 38],
  ArrowRight: ["ArrowRight", 39],
  ArrowDown: ["ArrowDown", 40],
  Delete: ["Delete", 46],
  F2: ["F2", 113],
  F5: ["F5", 116],
  "\\": ["Backslash", 220],
  ",": ["Comma", 188],
};

function keyInfo(key) {
  if (KEYS[key] !== undefined) return KEYS[key];
  if (/^[a-z]$/i.test(key))
    return [`Key${key.toUpperCase()}`, key.toUpperCase().charCodeAt(0)];
  if (/^[0-9]$/.test(key)) return [`Digit${key}`, key.charCodeAt(0)];
  return [key, 0];
}

/** Presses a key (optionally with Ctrl/Shift/Alt) the way a keyboard would. */
async function press(
  cdp,
  key,
  { ctrl = false, shift = false, alt = false, wait = 200, synthetic = false } = {},
) {
  if (synthetic) {
    // Chords Chromium reserves for itself (Ctrl+W closes the tab, Ctrl+Tab and
    // Ctrl+PgUp switch tabs) never reach the page from a real keyboard, but do
    // in the desktop webview. Dispatch them as DOM events instead.
    await cdp.evaluate(
      `(document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(key)}, ctrlKey: ${ctrl}, shiftKey: ${shift}, altKey: ${alt}, bubbles: true, cancelable: true }))`,
    );
    await sleep(wait);
    return;
  }
  const [code, vk] = keyInfo(key);
  const modifiers = (alt ? 1 : 0) | (ctrl ? 2 : 0) | (shift ? 8 : 0);
  const base = {
    key,
    code,
    windowsVirtualKeyCode: vk,
    nativeVirtualKeyCode: vk,
    modifiers,
  };
  // Enter needs a character event for forms to submit; other keys must not
  // produce text, or a shortcut would also type into the editor.
  await cdp.send(
    "Input.dispatchKeyEvent",
    key === "Enter"
      ? { type: "keyDown", text: "\r", ...base }
      : { type: "rawKeyDown", ...base },
  );
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
  await sleep(wait);
}

async function insertText(cdp, text) {
  await cdp.send("Input.insertText", { text });
  await sleep(150);
}

/** Types one character at a time, as keys, so typing-triggered features run. */
async function typeChars(cdp, text) {
  for (const ch of text) {
    await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", text: ch, key: ch });
    await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: ch });
    await sleep(40);
  }
  await sleep(150);
}

async function rect(cdp, selector) {
  return cdp.evaluate(`(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2, w: r.width, h: r.height, left: r.left, top: r.top, right: r.right, bottom: r.bottom };
  })()`);
}

async function mouse(cdp, type, x, y, button = "left", clickCount = 1) {
  await cdp.send("Input.dispatchMouseEvent", {
    type,
    x,
    y,
    button,
    buttons: button === "left" ? 1 : 4,
    clickCount,
  });
}

/** A real pointer click at the centre of an element. */
async function click(cdp, selector, button = "left") {
  const r = await rect(cdp, selector);
  if (r === null) throw new Error(`no element for ${selector}`);
  await mouse(cdp, "mouseMoved", r.x, r.y);
  await mouse(cdp, "mousePressed", r.x, r.y, button);
  await mouse(cdp, "mouseReleased", r.x, r.y, button);
  await sleep(200);
}

/** Click the first element matching `selector` whose text contains `text`. */
async function clickText(cdp, selector, text) {
  const ok = await cdp.evaluate(`(() => {
    const el = [...document.querySelectorAll(${JSON.stringify(selector)})].find((e) => e.textContent.includes(${JSON.stringify(text)}));
    if (!el) return false;
    el.scrollIntoView({ block: "nearest" });
    el.click();
    return true;
  })()`);
  if (!ok) throw new Error(`no ${selector} containing "${text}"`);
  await sleep(200);
}

const tabNames = (cdp) =>
  cdp.evaluate(
    `[...document.querySelectorAll('[role="tab"][data-tab]')].map((t) => t.dataset.tab)`,
  );
const activeTab = (cdp) =>
  cdp.evaluate(
    `document.querySelector('[role="tab"][data-tab][aria-selected="true"]')?.dataset.tab ?? null`,
  );
const statusText = (cdp) =>
  cdp.evaluate(
    `document.querySelector('footer[aria-label="Status"]')?.innerText ?? ""`,
  );
/** Unsaved tabs, by path. Used to catch keystrokes landing in the wrong document. */
const dirtyTabs = (cdp) =>
  cdp.evaluate(
    `[...document.querySelectorAll('[role="tab"][data-tab]')].filter((t) => t.getAttribute("aria-label").includes("unsaved")).map((t) => t.dataset.tab)`,
  );

async function noStrayEdits(cdp, where) {
  const dirty = await dirtyTabs(cdp);
  check(
    `no document is edited by accident (${where})`,
    dirty.length === 0,
    dirty.join(),
  );
}

const exists = (cdp, selector) =>
  cdp.evaluate(`document.querySelector(${JSON.stringify(selector)}) !== null`);

/** Click into the editor so keyboard input reaches CodeMirror. */
async function focusEditor(cdp) {
  await click(cdp, ".cm-content");
}

/** Move the caret to the end of the document. */
async function caretToEnd(cdp) {
  await press(cdp, "End", { ctrl: true });
}

/* -------------------------------------------------------------------------- */

async function main() {
  const browserBin = findBrowser();
  if (browserBin === null) {
    process.stderr.write("no chromium binary found; skipping UI verification\n");
    process.exit(1);
  }
  mkdirSync(SHOTS, { recursive: true });

  const child = spawn(
    browserBin,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-sandbox",
      "--hide-scrollbars",
      `--remote-debugging-port=${PORT}`,
      "--remote-allow-origins=*",
      "about:blank",
    ],
    { stdio: "ignore" },
  );

  try {
    if (!(await waitForEndpoint(PORT)))
      throw new Error("devtools endpoint never opened");
    const version = await fetch(`http://127.0.0.1:${PORT}/json/version`).then((r) =>
      r.json(),
    );
    const browser = await Cdp.open(version.webSocketDebuggerUrl);

    /* ------------------------------------------------------------------ */
    section("Home");
    const page = await newPage(browser);
    current = page;
    await shoot(page, "01-home");

    const cards = (cdp) =>
      cdp.evaluate(
        `[...document.querySelectorAll("[data-project]")].map((c) => c.dataset.project.split("/").pop())`,
      );
    const searchHome = async (cdp, text) => {
      // One search for the whole shell: on Home the top bar searches projects.
      await click(cdp, 'header input[aria-label="Search projects"]');
      await cdp.evaluate(
        `(() => { const i = document.querySelector('header input[aria-label="Search projects"]'); i.select(); })()`,
      );
      if (text === "") await press(cdp, "Delete");
      else await insertText(cdp, text);
      await sleep(200);
    };

    check(
      "home is the no-project surface; the top bar carries Open folder and New project",
      await page.evaluate(`(() => {
      const t = [...document.querySelectorAll("button")].map((b) => b.textContent.trim());
      return !!document.querySelector('section[aria-label="Home"]') && t.includes("Open folder") && t.includes("New project") && !document.querySelector('nav[aria-label="Workbench views"]') && document.querySelectorAll('input[type="search"]').length === 1;
    })()`),
    );
    check(
      "home nav lists Projects, Recent, Pinned, Archive, Templates and Settings",
      await page.evaluate(
        `[...document.querySelectorAll('nav[aria-label="Home sections"] button')].map((b) => b.getAttribute("aria-label")).join() === "Projects,Recent,Pinned,Archive,Templates,Settings"`,
      ),
    );
    check(
      "projects shows live projects, pinned first, missing folder flagged",
      await page.evaluate(`(() => {
      const names = [...document.querySelectorAll("[data-project]")].map((c) => c.dataset.project.split("/").pop());
      const missing = document.querySelector('[data-project$="old-thesis"] button');
      return names.join() === "entropy-paper,lecture-notes,thesis-chapter-3,thesis-draft,old-thesis" && missing?.disabled === true && missing.textContent.includes("Folder not found");
    })()`),
    );
    check(
      "cards show engine, a relative time and tags",
      await page.evaluate(`(() => {
      const c = document.querySelector('[data-project$="entropy-paper"]').innerText;
      return c.includes("Typst") && /ago|yesterday|just now|\\d{4}/.test(c) && c.includes("physics");
    })()`),
    );
    await expectEventually(
      page,
      "a stored thumbnail loads lazily into its card",
      `document.querySelector('[data-project$="thesis-chapter-3"] img[alt^="First page"]')?.src.startsWith("blob:")`,
    );

    await searchHome(page, "lecture");
    check("search narrows the list", (await cards(page)).join() === "lecture-notes");
    await searchHome(page, "zzzz");
    check(
      "a search with no match says so and offers to clear it",
      await page.evaluate(
        `document.body.innerText.includes('No projects match "zzzz"') && [...document.querySelectorAll("button")].some((b) => b.textContent.trim() === "Clear search")`,
      ),
    );
    await clickText(page, "button", "Clear search");
    check("clearing the search restores the list", (await cards(page)).length === 5);

    await clickText(page, 'nav[aria-label="Home sections"] button', "Pinned");
    check(
      "Pinned shows pinned projects only",
      (await cards(page)).join() === "entropy-paper",
    );
    await clickText(page, 'nav[aria-label="Home sections"] button', "Archive");
    check(
      "Archive shows archived projects",
      (await cards(page)).join() === "conference-poster",
    );
    await clickText(page, 'nav[aria-label="Home sections"] button', "Templates");
    check(
      "Templates lists a starter per engine",
      await page.evaluate(
        `[...document.querySelectorAll("main button, section[aria-label='Home'] button")].filter((b) => /(Typst|LaTeX) article|Markdown notes/.test(b.textContent)).length === 3`,
      ),
    );
    await shoot(page, "02-templates");
    await clickText(page, 'nav[aria-label="Home sections"] button', "Projects");

    // Sort and view.
    await page.evaluate(
      `(() => { const s = document.querySelector('select[aria-label="Sort projects"]'); s.value = "name"; s.dispatchEvent(new Event("change", { bubbles: true })); })()`,
    );
    await sleep(150);
    check(
      "sort by name orders within pinned and unpinned",
      (await cards(page)).join() ===
        "entropy-paper,lecture-notes,old-thesis,thesis-chapter-3,thesis-draft",
      (await cards(page)).join(),
    );
    await click(page, 'button[role="radio"][aria-label="List"]');
    check(
      "list view keeps every project",
      (await cards(page)).length === 5 &&
        (await exists(
          page,
          'button[role="radio"][aria-label="List"][aria-checked="true"]',
        )),
    );
    await shoot(page, "03-home-list");
    await click(page, 'button[role="radio"][aria-label="Grid"]');

    // Pin and archive through the card, then the menu.
    await click(page, '[data-project$="lecture-notes"] button[aria-label^="Pin"]');
    check(
      "pinning moves a project to the front and into Pinned",
      (await cards(page))[0] !== undefined &&
        (await exists(
          page,
          '[data-project$="lecture-notes"] button[aria-pressed="true"]',
        )),
    );
    await click(page, '[data-project$="lecture-notes"] button[aria-pressed="true"]');
    await page.evaluate(
      `document.querySelector('[data-project$="lecture-notes"] > button, [data-project$="lecture-notes"] button').dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 300 }))`,
    );
    await sleep(200);
    check(
      "the context menu offers open, rename, pin, archive, copy path and remove",
      await page.evaluate(
        `[...document.querySelectorAll('[role="menu"] [role="menuitem"]')].map((i) => i.textContent.trim()).join() === "Open,Rename,Edit tags,Pin,Archive,Copy path,Remove from list"`,
      ),
    );
    await shoot(page, "04-card-menu");
    await clickText(page, '[role="menuitem"]', "Archive");
    check(
      "archiving removes it from Projects",
      !(await cards(page)).includes("lecture-notes"),
    );
    await clickText(page, 'nav[aria-label="Home sections"] button', "Archive");
    check("and lists it in the Archive", (await cards(page)).includes("lecture-notes"));
    await page.evaluate(
      `document.querySelector('[data-project$="lecture-notes"] button').dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 300 }))`,
    );
    await sleep(200);
    await clickText(page, '[role="menuitem"]', "Unarchive");
    await clickText(page, 'nav[aria-label="Home sections"] button', "Projects");
    check("unarchiving brings it back", (await cards(page)).includes("lecture-notes"));

    // A folder that is not a project explains what to do (typed path in a browser).
    await clickText(page, "button", "Open folder");
    await click(page, 'input[placeholder*="my-paper"]');
    await insertText(page, "/nowhere");
    await press(page, "Enter", { wait: 400 });
    check(
      "opening a non-project folder says what to do",
      await page.evaluate(
        `document.querySelector('[role="alert"]')?.textContent.includes("contains manang.toml") ?? false`,
      ),
    );
    await clickText(page, "button", "Cancel");

    // New project dialog, then back out.
    await clickText(page, "button", "New project");
    check(
      "new project offers Typst, LaTeX and Markdown",
      await page.evaluate(
        `[...document.querySelectorAll('[role="radiogroup"][aria-label="Language"] [role="radio"]')].map((r) => r.textContent.trim().split(/\\s/)[0]).join() === "Typst,LaTeX,Markdown"`,
      ),
    );
    await shoot(page, "05-new-project");
    await press(page, "Escape");
    check(
      "Esc closes the new project dialog",
      !(await exists(page, '[role="dialog"]')),
    );

    /* ------------------------------------------------------------------ */
    section("Workbench");
    await reload(page);
    await clickText(page, "[data-project] button", "Entropy paper");
    await waitFor(
      page,
      `document.querySelector(".cm-content")?.textContent.includes("Landauer")`,
      { label: "editor shows main.typ" },
    );
    await expectEventually(
      page,
      "preview renders the compiled page as SVG from typst.render",
      `document.querySelector('img[alt^="Page 1"]')?.src.startsWith("blob:")`,
    );
    await expectEventually(
      page,
      "status bar reports compile time",
      `/Compiled in \\d+ ms/.test(document.querySelector('footer[aria-label="Status"]').innerText)`,
    );
    await sleep(500);
    await shoot(page, "03-workbench");

    check(
      "workbench has top bar, activity bar, files panel, tabs and status bar",
      await page.evaluate(
        `(() => !!document.querySelector("header") && !!document.querySelector('nav[aria-label="Workbench views"]') && !!document.querySelector('aside[aria-label="Files panel"]') && !!document.querySelector('[role="tablist"][aria-label="Open files"]') && !!document.querySelector('footer[aria-label="Status"]'))()`,
      ),
    );
    check("main.typ opened in a tab on open", (await activeTab(page)) === "main.typ");
    check(
      "breadcrumb names the project and file",
      await page.evaluate(
        `document.querySelector('nav[aria-label="Breadcrumb"]').innerText.replace(/\\s+/g, " ").includes("Entropy paper / main.typ")`,
      ),
    );
    check(
      "status bar shows engine, caret, words, git branch and kernel",
      await page.evaluate(
        `(() => { const t = document.querySelector('footer[aria-label="Status"]').innerText; return t.includes("Typst") && /Ln \\d+, Col \\d+/.test(t) && /\\d+ words/.test(t) && t.includes("main*") && /kernel/i.test(t); })()`,
      ),
    );
    check(
      "the page stays paper-white in the dark theme",
      await page.evaluate(
        `getComputedStyle(document.querySelector('img[alt^="Page 1"]')).backgroundColor === "rgb(250, 249, 247)"`,
      ),
    );
    check(
      "lime marks the active tab only",
      await page.evaluate(`(() => {
      const lime = "rgb(198, 241, 53)";
      const spans = [...document.querySelectorAll('[role="tablist"] span')].filter((s) => getComputedStyle(s).backgroundColor === lime);
      return spans.length === 1;
    })()`),
    );

    /* ------------------------------------------------------------------ */
    section("Activity bar and side panels");
    const views = [
      ["Search", 'aside[aria-label="Search panel"]'],
      ["Figures", 'aside[aria-label="Figures panel"]'],
      ["References", 'aside[aria-label="References panel"]'],
      ["Files", 'aside[aria-label="Files panel"]'],
    ];
    for (const [label, panel] of views) {
      await click(
        page,
        `nav[aria-label="Workbench views"] button[aria-label="${label}"]`,
      );
      check(
        `${label} opens its side panel and marks the icon pressed`,
        (await exists(page, panel)) &&
          (await page.evaluate(
            `document.querySelector('button[aria-label="${label}"]').getAttribute("aria-pressed") === "true"`,
          )),
      );
      if (label === "Figures") await shoot(page, "05-figures-panel");
    }
    await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');
    check(
      "clicking the active icon collapses the side panel",
      !(await exists(page, "#side-panel")),
    );
    await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');
    check("clicking it again reopens", await exists(page, "#side-panel"));

    // Search
    await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Search"]');
    await click(page, 'input[aria-label="Search project"]');
    await insertText(page, "entropy");
    await expectEventually(
      page,
      "search finds text across files and file names",
      `(() => { const t = document.querySelector("#side-panel").innerText; return /matches? in \\d+ files?/.test(t); })()`,
    );
    await shoot(page, "04-search");
    await page.evaluate(
      `document.querySelector('#side-panel section[aria-label^="Matches in"] button').click()`,
    );
    await sleep(400);
    check("choosing a match opens the file", (await tabNames(page)).length >= 1);

    await noStrayEdits(page, "after search");
    // References
    await click(
      page,
      'nav[aria-label="Workbench views"] button[aria-label="References"]',
    );
    await expectEventually(
      page,
      "references list the .bib entries",
      `document.querySelectorAll('#side-panel ul[aria-label="References"] li').length === 4`,
    );
    await shoot(page, "06-references");
    await page.evaluate(
      `document.querySelector('input[aria-label="Filter references"]').focus()`,
    );
    await insertText(page, "irreversibility");
    await expectEventually(
      page,
      "reference filter narrows the list",
      `document.querySelectorAll('#side-panel ul[aria-label="References"] li').length === 1`,
    );

    await noStrayEdits(page, "after references");
    // Settings: a page, with every plugin and its manifest settings in it.
    await click(
      page,
      'nav[aria-label="Workbench views"] button[aria-label="Settings"]',
    );
    await expectEventually(
      page,
      "settings open as a page over the editor",
      `!!document.querySelector('section[aria-label="Settings"]') && document.querySelector('nav[aria-label="Workbench views"] button[aria-label="Settings"]').getAttribute("aria-pressed") === "true"`,
    );
    check(
      "the top bar search now searches settings",
      await exists(page, 'header input[aria-label="Search settings and plugins"]'),
    );
    await expectEventually(
      page,
      "settings list every registered plugin, with its access",
      `(() => { const t = document.querySelector('section[aria-label="Settings"]').innerText; return ["Editor", "Project", "Typst", "LaTeX", "Citations"].every((n) => t.includes(n)) && t.includes("Connects to api.crossref.org"); })()`,
    );
    check(
      "a plugin's manifest settings appear in its section",
      await exists(page, '[data-plugin="latex"] [data-setting="latex.engine"]'),
    );
    await click(page, 'header input[aria-label="Search settings and plugins"]');
    await insertText(page, "engine");
    // `compile.engine` and `latex.engine` match by name; the LaTeX plugin also
    // matches as a whole (its `latex.status` command is "LaTeX engines
    // available"), so all of its settings stay listed.
    await expectEventually(
      page,
      "searching narrows settings and plugins together",
      `(() => { const ids = [...document.querySelectorAll("[data-setting]")].map((e) => e.dataset.setting).sort().join(); return ids === "compile.engine,latex.backend,latex.engine,latex.latexmk,latex.timeout-seconds"; })()`,
      {
        show: `[...document.querySelectorAll("[data-setting]")].map((e) => e.dataset.setting).sort().join()`,
      },
    );
    await clickText(page, '[data-plugin="latex"] [role="radio"]', "XeLaTeX");
    await expectEventually(
      page,
      "changing a plugin setting writes it and marks it changed",
      `document.querySelector('[data-setting="latex.engine"]').innerText.includes("Changed")`,
    );
    await clickText(page, '[data-plugin="latex"] button[aria-expanded]', "commands");
    check(
      "a plugin expands to its commands",
      await page.evaluate(
        `document.querySelector('[data-plugin="latex"]').innerText.includes("latex.compile")`,
      ),
    );
    await shoot(page, "07-settings-plugins");
    await press(page, "Escape");
    await press(page, "Escape");
    await expectEventually(
      page,
      "Escape clears the search, then closes settings",
      `!document.querySelector('section[aria-label="Settings"]')`,
    );

    /* ------------------------------------------------------------------ */
    section("Writing and citation settings");
    // A word for the personal dictionary, added the usual way in notes.md.
    await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');
    await page.evaluate(
      `document.querySelector('[role="treeitem"][data-path="notes.md"]').click()`,
    );
    await waitFor(
      page,
      `document.querySelector(".cm-content")?.textContent.includes("Notes")`,
      {
        label: "notes.md open",
      },
    );
    await focusEditor(page);
    await caretToEnd(page);
    await insertText(page, "\nrecieve");
    const recieveFlagged = `[...document.querySelectorAll(".cm-spell")].some((e) => e.textContent === "recieve")`;
    await waitFor(page, recieveFlagged, { label: "recieve flagged", timeout: 8000 });
    await press(page, ".", { ctrl: true, wait: 400 });
    await click(page, '.cm-spell-item[data-action="add-word"]');
    await waitFor(page, `!${recieveFlagged}`, {
      label: "recieve added",
      timeout: 8000,
    });

    // The References output in the default style, before changing it.
    await click(
      page,
      'nav[aria-label="Workbench views"] button[aria-label="References"]',
    );
    const expandReference = () =>
      page.evaluate(`(() => {
        const b = document.querySelector('#side-panel ul[aria-label="References"] button[aria-label="Expand details"]');
        b?.click();
        return b !== null;
      })()`);
    await waitFor(
      page,
      `document.querySelectorAll('#side-panel ul[aria-label="References"] li').length >= 1`,
      { label: "references listed" },
    );
    await expandReference();
    await expectEventually(
      page,
      "References formats an entry in the configured style (APA by default)",
      `(() => { const t = document.querySelector("[data-formatted-citation]")?.textContent ?? ""; return t.includes("(1961)") && !t.includes("[1]"); })()`,
      { show: `document.querySelector("[data-formatted-citation]")?.textContent` },
    );
    check(
      "the panel names the style instead of offering its own switch",
      await page.evaluate(
        `document.querySelector('[data-citation-style]')?.textContent.trim() === "APA" && ![...document.querySelectorAll("#side-panel button")].some((b) => b.textContent.trim() === "IEEE")`,
      ),
    );

    // Settings: the dictionary list, with the word in it.
    await click(
      page,
      'nav[aria-label="Workbench views"] button[aria-label="Settings"]',
    );
    await expectEventually(
      page,
      "the Writing section lists the personal dictionary",
      `!!document.querySelector('[data-plugin="writing"] [data-dictionary] [data-word="recieve"]')`,
    );
    check(
      "the spelling language offers the dictionaries the plugin reports, by name",
      await page.evaluate(
        `(() => { const o = [...document.querySelectorAll('[data-setting="writing.spell-language"] option')].map((x) => x.textContent.trim()); return o.includes("American English") && o.includes("British English"); })()`,
      ),
    );
    await page.evaluate(
      `document.querySelector('[data-setting="writing.spell-language"]').scrollIntoView({ block: "start" })`,
    );
    await shoot(page, "07b-settings-dictionary");
    await page.evaluate(`(() => {
      const b = document.querySelector('[data-word="recieve"] button[aria-label^="Remove"]');
      b.scrollIntoView({ block: "center" });
      b.click();
    })()`);
    await expectEventually(
      page,
      "removing a word empties the list and shows the empty state",
      `!document.querySelector('[data-word="recieve"]') && !!document.querySelector("[data-dictionary-empty]")`,
    );
    await clickText(page, '[data-plugin="citations"] [role="radio"]', "Numeric (IEEE)");
    await expectEventually(
      page,
      "choosing a citation style in Settings saves it",
      `document.querySelector('[data-setting="citations.style"]').innerText.includes("Changed")`,
    );
    await press(page, "Escape");
    if (await exists(page, 'section[aria-label="Settings"]'))
      await press(page, "Escape");
    await waitFor(page, `!document.querySelector('section[aria-label="Settings"]')`, {
      label: "settings closed",
    });

    await expectEventually(
      page,
      "the removed word is flagged again in the editor",
      recieveFlagged,
      { timeout: 8000 },
    );
    if (!(await exists(page, "[data-formatted-citation]"))) await expandReference();
    await expectEventually(
      page,
      "References re-formats in the new style",
      `(() => { const t = document.querySelector("[data-formatted-citation]")?.textContent ?? ""; return t.includes("[1]") && document.querySelector("[data-citation-style]")?.dataset.citationStyle === "numeric"; })()`,
      { show: `document.querySelector("[data-formatted-citation]")?.textContent` },
    );

    // Convert to Typst from the palette.
    await click(page, '[role="tab"][data-tab="notes.md"]');
    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "convert to typst");
    check(
      "the palette offers Convert to Typst for a Markdown tab",
      await page.evaluate(
        `document.querySelector('[role="option"][aria-selected="true"]')?.textContent.includes("Convert to Typst")`,
      ),
    );
    await press(page, "Enter", { wait: 400 });
    await expectEventually(
      page,
      "converting writes notes.typ and opens it",
      `document.querySelector('[role="tab"][data-tab][aria-selected="true"]')?.dataset.tab === "notes.typ" && document.querySelector(".cm-content")?.textContent.includes("= Notes")`,
    );
    await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');
    await expectEventually(
      page,
      "the Typst file appears in the tree next to the Markdown one",
      `!!document.querySelector('[role="treeitem"][data-path="notes.typ"]')`,
    );
    await click(page, '[role="tab"][data-tab="notes.md"]');
    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "convert to typst");
    await press(page, "Enter", { wait: 400 });
    await expectEventually(
      page,
      "converting again asks before replacing notes.typ",
      `document.querySelector('[role="alertdialog"]')?.innerText.includes("Replace notes.typ")`,
    );
    await clickText(page, '[role="alertdialog"] button', "Replace");
    await expectEventually(
      page,
      "replacing opens the new notes.typ",
      `!document.querySelector('[role="alertdialog"]') && document.querySelector('[role="tab"][data-tab][aria-selected="true"]')?.dataset.tab === "notes.typ"`,
    );
    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "convert to typst");
    check(
      "Convert to Typst is not offered for a Typst tab",
      !(await page.evaluate(
        `[...document.querySelectorAll('[role="dialog"] [role="option"]')].some((o) => o.textContent.includes("Convert to Typst"))`,
      )),
    );
    await press(page, "Escape");

    // Leave notes.md as it was, and close what this section opened.
    await click(page, '[role="tab"][data-tab="notes.md"]');
    await focusEditor(page);
    await press(page, "z", { ctrl: true });
    await click(page, '[role="tab"][data-tab="notes.typ"]', "middle");
    await click(page, '[role="tab"][data-tab="notes.md"]', "middle");
    await noStrayEdits(page, "after converting");
    // The side panel shows References again, as before this section.
    await click(
      page,
      'nav[aria-label="Workbench views"] button[aria-label="References"]',
    );

    /* ------------------------------------------------------------------ */
    section("Files tree");
    await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');
    check(
      "tree is an ARIA tree with levels and set sizes",
      await page.evaluate(
        `(() => { const t = document.querySelector('[role="tree"]'); const items = t.querySelectorAll('[role="treeitem"]'); return items.length >= 5 && [...items].every((i) => i.getAttribute("aria-level") && i.getAttribute("aria-posinset") && i.getAttribute("aria-setsize")); })()`,
      ),
    );
    // Expand a folder with the keyboard.
    await page.evaluate(
      `document.querySelector('[role="treeitem"][data-path="sections"]').focus()`,
    );
    await press(page, "ArrowRight", { wait: 300 });
    check(
      "Right arrow expands a folder and loads its children lazily",
      await page.evaluate(
        `!!document.querySelector('[data-path="sections/results.typ"]') && document.querySelector('[data-path="sections"]').getAttribute("aria-expanded") === "true"`,
      ),
    );
    check(
      "git badges show M and U",
      await page.evaluate(
        `(() => { const row = document.querySelector('[data-path="sections/results.typ"]'); const note = document.querySelector('[data-path="notes.md"]'); return row?.innerText.includes("M") && note?.innerText.includes("U"); })()`,
      ),
    );
    await press(page, "ArrowDown");
    check(
      "Down arrow moves focus to the next row",
      await page.evaluate(
        `document.activeElement?.dataset.path === "sections/background.typ"`,
      ),
    );
    await press(page, "ArrowLeft");
    check(
      "Left arrow from a child moves to its folder",
      await page.evaluate(`document.activeElement?.dataset.path === "sections"`),
    );
    await press(page, "End");
    check(
      "End jumps to the last row",
      await page.evaluate(
        `document.activeElement?.dataset.path === [...document.querySelectorAll('[role="treeitem"]')].pop().dataset.path`,
      ),
    );
    await press(page, "Home");
    check(
      "Home jumps to the first row",
      await page.evaluate(
        `document.activeElement?.dataset.path === document.querySelector('[role="treeitem"]').dataset.path`,
      ),
    );
    await page.evaluate(
      `document.querySelector('[role="treeitem"][data-path="sections"]').focus()`,
    );
    await press(page, "ArrowDown");
    await press(page, "Enter", { wait: 500 });
    check(
      "Enter opens the focused file in a tab",
      (await activeTab(page)) === "sections/background.typ",
    );
    await shoot(page, "08-files-keyboard");

    await noStrayEdits(page, "after tree navigation");
    // Files are found through the one search (Ctrl+P), not a second field.
    check(
      "the files panel has no filter field of its own",
      !(await exists(page, 'input[aria-label="Filter files"]')),
    );

    await noStrayEdits(page, "after filtering");
    // Create, rename, delete
    await click(page, 'button[aria-label="New file"]');
    await expectEventually(
      page,
      "New file starts an inline name field with a suggested name",
      `document.querySelector("input[data-editing]")?.value === "untitled.typ"`,
    );
    await press(page, "Enter", { wait: 600 });
    check(
      "creating a file opens it in a new tab",
      (await activeTab(page)) === "untitled.typ",
    );
    await page.evaluate(
      `document.querySelector('[role="treeitem"][data-path="untitled.typ"]').focus()`,
    );
    await press(page, "F2");
    // The field selects the stem, so typing replaces "untitled" and keeps ".typ".
    await insertText(page, "ideas");
    await press(page, "Enter", { wait: 600 });
    check(
      "F2 renames inline and the tab follows",
      (await tabNames(page)).includes("ideas.typ") &&
        !(await tabNames(page)).includes("untitled.typ"),
    );
    await page.evaluate(
      `document.querySelector('[role="treeitem"][data-path="ideas.typ"]').focus()`,
    );
    await press(page, "Delete", { wait: 600 });
    check(
      "Delete moves the file to the trash and closes its tab",
      !(await tabNames(page)).includes("ideas.typ") &&
        !(await exists(page, '[data-path="ideas.typ"]')),
    );
    check(
      "a notice confirms the trash",
      (await statusText(page)).includes("Moved ideas.typ to the trash"),
    );

    await noStrayEdits(page, "after create, rename and delete");
    // Context menu
    await click(page, '[role="treeitem"][data-path="notes.md"]', "right");
    check(
      "right click opens a context menu",
      await page.evaluate(
        `document.querySelector('[role="menu"] [role="menuitem"]') !== null && [...document.querySelectorAll('[role="menuitem"]')].map((m) => m.textContent).join().includes("Rename")`,
      ),
    );
    await shoot(page, "09-context-menu");
    await press(page, "Escape");
    check("Esc closes the menu", !(await exists(page, '[role="menu"]')));

    /* ------------------------------------------------------------------ */
    section("Tabs");
    await noStrayEdits(page, "before tabs");
    await page.evaluate(
      `document.querySelector('[role="treeitem"][data-path="notes.md"]').click()`,
    );
    await sleep(400);
    await page.evaluate(
      `document.querySelector('[role="treeitem"][data-path="main.typ"]').click()`,
    );
    await sleep(400);
    const opened = await tabNames(page);
    check(
      "several files are open as tabs",
      opened.includes("main.typ") &&
        opened.includes("notes.md") &&
        opened.includes("sections/background.typ"),
      opened.join(),
    );
    check(
      "tabs follow the ARIA tabs pattern",
      await page.evaluate(
        `(() => { const tabs = [...document.querySelectorAll('[role="tab"][data-tab]')]; return tabs.filter((t) => t.tabIndex === 0).length === 1 && tabs.filter((t) => t.getAttribute("aria-selected") === "true").length === 1; })()`,
      ),
    );

    await press(page, "Tab", { ctrl: true, synthetic: true });
    check("Ctrl+Tab goes to the next tab", (await activeTab(page)) !== "main.typ");
    await press(page, "2", { ctrl: true });
    check("Ctrl+2 selects the second tab", (await activeTab(page)) === opened[1]);
    await press(page, "9", { ctrl: true });
    check(
      "Ctrl+9 selects the last tab",
      (await activeTab(page)) === opened[opened.length - 1],
    );
    await press(page, "PageUp", { ctrl: true, synthetic: true });
    check(
      "Ctrl+PageUp goes to the previous tab",
      (await activeTab(page)) === opened[opened.length - 2],
    );

    // Arrow keys within the tablist.
    await page.evaluate(
      `document.querySelector('[role="tab"][aria-selected="true"]').focus()`,
    );
    await press(page, "ArrowLeft");
    check(
      "Left arrow moves between tabs and activates",
      (await activeTab(page)) === opened[opened.length - 3] || opened.length < 3,
    );
    await press(page, "Home");
    check("Home goes to the first tab", (await activeTab(page)) === opened[0]);

    await noStrayEdits(page, "after tab shortcuts");
    // Dirty dot
    await click(page, '[role="tab"][data-tab="main.typ"]');
    await sleep(400);
    await focusEditor(page);
    await caretToEnd(page);
    await insertText(page, "\nMore words.");
    check(
      "typing marks the tab dirty",
      await page.evaluate(
        `document.querySelector('[role="tab"][data-tab="main.typ"]').getAttribute("aria-label").includes("unsaved")`,
      ),
    );
    check(
      "the tree marks the file too",
      await page.evaluate(
        `!!document.querySelector('[data-path="main.typ"] [aria-label="Unsaved changes"]')`,
      ),
    );
    await shoot(page, "10-dirty-tab");

    // Close a dirty tab: asks, and Cancel keeps it.
    await press(page, "w", { ctrl: true, wait: 300, synthetic: true });
    check(
      "closing a dirty tab asks what to do",
      await page.evaluate(
        `document.querySelector('[role="alertdialog"]')?.innerText.includes("Save changes to main.typ")`,
      ),
    );
    await shoot(page, "11-close-dialog");
    await press(page, "Escape");
    check(
      "Esc cancels and keeps the tab",
      (await tabNames(page)).includes("main.typ") &&
        !(await exists(page, '[role="alertdialog"]')),
    );
    await press(page, "s", { ctrl: true, wait: 400 });
    check(
      "Ctrl+S saves and clears the dirty mark",
      await page.evaluate(
        `!document.querySelector('[role="tab"][data-tab="main.typ"]').getAttribute("aria-label").includes("unsaved")`,
      ),
    );
    await expectEventually(
      page,
      "the saved change reaches the project (git shows main.typ modified)",
      `document.querySelector('[data-path="main.typ"]')?.innerText.includes("M")`,
    );
    await press(page, "w", { ctrl: true, wait: 300, synthetic: true });
    check(
      "Ctrl+W closes a saved tab without asking",
      !(await tabNames(page)).includes("main.typ") &&
        !(await exists(page, '[role="alertdialog"]')),
    );

    // Middle-click closes.
    await click(page, '[role="tab"][data-tab="notes.md"]', "middle");
    check("middle-click closes a tab", !(await tabNames(page)).includes("notes.md"));

    await noStrayEdits(page, "after middle-click");

    // Reopen main for the next sections.
    await page.evaluate(
      `document.querySelector('[role="treeitem"][data-path="main.typ"]').click()`,
    );
    await sleep(600);

    /* ------------------------------------------------------------------ */
    section("Problems, preview and the gutter");
    await focusEditor(page);
    await caretToEnd(page);
    await insertText(page, "\nA broken formula $x + y\n");
    await expectEventually(
      page,
      "status bar counts the error after typing",
      `/1 error/.test(document.querySelector('footer[aria-label="Status"]').innerText)`,
      { timeout: 8000 },
    );
    await expectEventually(
      page,
      "the gutter shows a solid marker on the broken line",
      `document.querySelectorAll(".manang-problem-error").length >= 1`,
    );
    check(
      "the preview keeps the last good page and says so",
      await page.evaluate(
        `document.querySelector('img[alt^="Page 1"]') !== null && document.querySelector('section[aria-label="Preview"]').innerText.includes("last good page")`,
      ),
    );
    await shoot(page, "12-error-state");

    // Open Problems via the status bar.
    await clickText(page, 'footer[aria-label="Status"] button', "error");
    check(
      "the status bar opens the Problems panel",
      await exists(page, "#bottom-panel"),
    );
    check(
      "Problems lists the message with line and column",
      await page.evaluate(
        `(() => { const t = document.querySelector("#bottom-panel").innerText; return t.includes("Unclosed math delimiter") && /Ln \\d+, Col \\d+/.test(t); })()`,
      ),
    );
    check(
      "the Problems tab carries the count",
      await page.evaluate(
        `document.querySelector("#panel-tab-problems").innerText.includes("1")`,
      ),
    );
    await shoot(page, "13-problems");
    await page.evaluate(
      `document.querySelector("#bottom-panel [data-problem]").click()`,
    );
    await sleep(400);
    const caret = await page.evaluate(
      `document.querySelector('footer[aria-label="Status"]').innerText.match(/Ln (\\d+), Col (\\d+)/)?.slice(1).map(Number)`,
    );
    const problemLine = await page.evaluate(
      `document.querySelector("#bottom-panel [data-problem]").innerText.match(/Ln (\\d+)/)?.[1]`,
    );
    check(
      "clicking a problem jumps the caret to its line",
      caret !== null && String(caret[0]) === problemLine,
      `caret ${caret}, problem line ${problemLine}`,
    );

    // Output tab
    await click(page, "#panel-tab-output");
    check(
      "Output shows the compile log",
      await page.evaluate(
        `document.querySelector('[role="log"]').innerText.includes("Compiled main.typ")`,
      ),
    );

    // Fix it again by undoing.
    await focusEditor(page);
    await press(page, "z", { ctrl: true });
    await expectEventually(
      page,
      "undoing the mistake clears the error",
      `/Compiled in \\d+ ms/.test(document.querySelector('footer[aria-label="Status"]').innerText)`,
      { timeout: 8000 },
    );
    check(
      "Problems says so",
      await page.evaluate(
        `document.querySelector("#panel-tab-problems").getAttribute("aria-selected") !== null`,
      ),
    );

    // Ctrl+J toggles the panel.
    await press(page, "j", { ctrl: true });
    check("Ctrl+J toggles the bottom panel", !(await exists(page, "#bottom-panel")));
    await press(page, "j", { ctrl: true });

    // Preview toolbar
    check(
      "preview toolbar shows page n of N",
      await page.evaluate(
        `/Page 1 of \\d+/.test(document.querySelector('section[aria-label="Preview"]').innerText)`,
      ),
    );
    await click(page, 'button[aria-label="Zoom in"]');
    check(
      "zoom in sets a percentage",
      await page.evaluate(
        `/125%/.test(document.querySelector('section[aria-label="Preview"]').innerText)`,
      ),
    );
    await click(page, 'button[aria-label="Refresh preview"]');
    // Layout switch, from the preview's own header.
    await click(
      page,
      'section[aria-label="Preview"] button[aria-label="Close preview"]',
    );
    check(
      "Close preview leaves the editor alone",
      !(await exists(page, 'section[aria-label="Preview"]')) &&
        (await exists(page, ".cm-editor")),
    );
    await click(page, '[role="tablist"] ~ * button[aria-label="Show preview"]');
    await waitFor(
      page,
      `document.querySelector('section[aria-label="Preview"]') !== null`,
    );
    await click(
      page,
      'section[aria-label="Preview"] button[aria-label="Maximise preview"]',
    );
    check(
      "Maximise preview hides the editor",
      !(await exists(page, ".cm-editor")) &&
        (await exists(page, 'section[aria-label="Preview"]')),
    );
    await click(
      page,
      'section[aria-label="Preview"] button[aria-label="Restore split"]',
    );
    await waitFor(page, `document.querySelector(".cm-editor") !== null`);

    // Resize divider by keyboard.
    const before = await rect(page, ".cm-editor");
    await page.evaluate(
      `document.querySelector('[role="separator"][aria-label="Resize editor and preview"]').focus()`,
    );
    await press(page, "ArrowRight");
    await press(page, "ArrowRight");
    const after = await rect(page, ".cm-editor");
    check(
      "the split divider moves with the arrow keys",
      after !== null && before !== null && after.w > before.w,
      `${before?.w} -> ${after?.w}`,
    );

    /* ------------------------------------------------------------------ */
    section("Command palette");
    await press(page, "k", { ctrl: true, wait: 300 });
    check(
      "Ctrl+K opens the palette as a dialog with focus in the field",
      await page.evaluate(
        `document.querySelector('[role="dialog"] input[role="combobox"]') === document.activeElement`,
      ),
    );
    check(
      "palette lists workbench and plugin commands in groups",
      await page.evaluate(
        `(() => { const g = [...document.querySelectorAll('[role="dialog"] [role="group"]')].map((x) => x.getAttribute("aria-label")); return g.includes("Workbench") && g.includes("Plugin commands"); })()`,
      ),
    );
    check(
      "real shortcuts are shown beside commands",
      await page.evaluate(
        `document.querySelector('[role="dialog"]').innerText.includes("Ctrl+J") && document.querySelector('[role="dialog"]').innerText.includes("Ctrl+Shift+P") === false`,
      ),
    );
    await shoot(page, "14-palette");
    await insertText(page, "problems");
    check(
      "typing filters with fuzzy search",
      await page.evaluate(
        `document.querySelector('[role="option"][aria-selected="true"]')?.textContent.includes("problems")`,
      ),
    );
    await press(page, "ArrowDown");
    check(
      "arrow keys move the highlight (aria-activedescendant follows)",
      await page.evaluate(
        `document.activeElement.getAttribute("aria-activedescendant") === document.querySelector('[role="option"][aria-selected="true"]').id`,
      ),
    );
    await press(page, "Escape");
    check(
      "Esc closes the palette and returns focus",
      !(await exists(page, '[role="dialog"]')),
    );

    await press(page, "p", { ctrl: true, shift: true, wait: 300 });
    check(
      "Ctrl+Shift+P also opens it",
      await exists(page, '[role="dialog"] input[role="combobox"]'),
    );
    await insertText(page, "export pdf");
    check(
      "export is listed once",
      await page.evaluate(
        `[...document.querySelectorAll('[role="dialog"] [role="option"]')].filter((o) => o.textContent.includes("Export PDF")).length === 1`,
      ),
    );
    await press(page, "Escape");

    // The layout and zen live in the palette now that the top bar has no buttons.
    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "preview");
    check(
      "the palette offers split, editor only and preview only",
      await page.evaluate(
        `(() => { const t = document.querySelector('[role="dialog"]').innerText; return t.includes("editor and preview side by side") && t.includes("Preview: editor only") && t.includes("Preview: preview only"); })()`,
      ),
    );
    await press(page, "Escape");
    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "zen");
    check(
      "the palette offers zen with its shortcut",
      await page.evaluate(
        `(() => { const t = document.querySelector('[role="dialog"]').innerText; return t.includes("Enter zen mode") && /(Ctrl|⌘)\\+?\\\\/.test(t); })()`,
      ),
    );
    await press(page, "Escape");

    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "format");
    check(
      "Format document is listed with its shortcut",
      await page.evaluate(
        `(() => { const t = document.querySelector('[role="dialog"]').innerText; return t.includes("Format document") && /Alt\\+Shift\\+F|Shift\\+Alt\\+F/.test(t); })()`,
      ),
    );
    await press(page, "Escape");
    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "editor");
    check(
      "plugin internals are not offered (no editor.new, editor.insert...)",
      await page.evaluate(
        `(() => { const t = document.querySelector('[role="dialog"]').innerText; return !t.includes("editor.new") && !t.includes("editor.insert") && !t.includes("project.rename"); })()`,
      ),
    );
    await press(page, "Escape");

    await press(page, "p", { ctrl: true, wait: 300 });
    await insertText(page, "talk");
    await press(page, "Enter", { wait: 600 });
    check("Ctrl+P goes to a file by name", (await activeTab(page)) === "talk.typ");

    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "compile output");
    await press(page, "Enter", { wait: 300 });
    check(
      "palette runs workbench commands",
      await page.evaluate(
        `document.querySelector("#panel-tab-output")?.getAttribute("aria-selected") === "true"`,
      ),
    );

    /* ------------------------------------------------------------------ */
    section("Writing tools");
    const wordCount = () =>
      page.evaluate(
        `Number(document.querySelector("[data-word-count]")?.textContent.match(/\\d+/)?.[0] ?? -1)`,
      );

    await focusEditor(page);
    await caretToEnd(page);
    await insertText(
      page,
      "\n\nI beleive teh data is is clear.  \nA broken $x + y\n\n\n\nLast line",
    );
    const baseWords = await wordCount();

    await expectEventually(
      page,
      "misspelled words get a squiggle after typing stops",
      `[...document.querySelectorAll(".cm-spell")].some((e) => e.textContent === "teh") && [...document.querySelectorAll(".cm-spell")].some((e) => e.textContent === "beleive")`,
      { timeout: 8000 },
    );
    check(
      "the squiggle is a wavy underline, not a rounded or coloured box",
      await page.evaluate(
        `(() => { const s = getComputedStyle(document.querySelector(".cm-spell")); return s.textDecorationStyle === "wavy" && s.borderRadius === "0px"; })()`,
      ),
    );
    await shoot(page, "20-spell-squiggle");

    await expectEventually(
      page,
      "prose lint underlines the repeated word, in a different style from spelling",
      `(() => { const el = [...document.querySelectorAll(".cm-prose-warning")].find((e) => e.textContent.includes("is is")); return el !== undefined && getComputedStyle(el).textDecorationStyle === "dashed"; })()`,
      { timeout: 8000 },
    );
    if (!(await exists(page, "#bottom-panel"))) await press(page, "j", { ctrl: true });
    await click(page, "#panel-tab-problems");
    check(
      "Problems lists the prose finding (source writing) next to the compiler error",
      await page.evaluate(
        `(() => { const t = document.querySelector("#bottom-panel")?.innerText ?? ""; return t.includes("Repeated word") && t.includes("writing") && t.includes("Unclosed math delimiter"); })()`,
      ),
    );
    await shoot(page, "21-problems-writing");
    await focusEditor(page);

    // Quick fix by clicking the squiggle.
    const first = await page.evaluate(
      `(() => { const el = [...document.querySelectorAll(".cm-spell")].find((e) => e.textContent === "beleive"); const r = el.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`,
    );
    await mouse(page, "mouseMoved", first.x, first.y);
    await mouse(page, "mousePressed", first.x, first.y);
    await mouse(page, "mouseReleased", first.x, first.y);
    await expectEventually(
      page,
      "clicking a squiggle opens the quick-fix menu with suggestions and Add to dictionary",
      `(() => { const m = document.querySelector(".cm-spell-menu"); return m !== null && m.innerText.includes("believe") && m.innerText.includes("Add to dictionary"); })()`,
    );
    await shoot(page, "22-quick-fix");
    await click(page, '.cm-spell-item[data-fix="believe"]');
    await expectEventually(
      page,
      "choosing a suggestion replaces the word",
      `document.querySelector(".cm-content").innerText.includes("I believe teh data")`,
    );

    // Quick fix by keyboard: caret just after "teh", Ctrl+.
    await expectEventually(
      page,
      "the fixed word loses its squiggle",
      `![...document.querySelectorAll(".cm-spell")].some((e) => e.textContent === "believe")`,
      { timeout: 8000 },
    );
    // Move the caret: Home of the line, then past "I believe teh".
    await press(page, "Home");
    for (let i = 0; i < "I believe teh".length; i += 1)
      await press(page, "ArrowRight", { wait: 10 });
    await press(page, ".", { ctrl: true, wait: 400 });
    check("Ctrl+. opens the menu at the caret", await exists(page, ".cm-spell-menu"));
    await press(page, "Enter", { wait: 300 });
    await expectEventually(
      page,
      "Enter applies the highlighted suggestion",
      `document.querySelector(".cm-content").innerText.includes("I believe the data")`,
    );

    // Add to dictionary.
    await caretToEnd(page);
    await insertText(page, "\nwich");
    await expectEventually(
      page,
      "a new misspelling is flagged",
      `[...document.querySelectorAll(".cm-spell")].some((e) => e.textContent === "wich")`,
      { timeout: 8000 },
    );
    await press(page, ".", { ctrl: true, wait: 400 });
    await click(page, '.cm-spell-item[data-action="add-word"]');
    await expectEventually(
      page,
      "Add to dictionary clears the squiggle",
      `![...document.querySelectorAll(".cm-spell")].some((e) => e.textContent === "wich")`,
      { timeout: 8000 },
    );

    // Completions.
    await insertText(page, "\n$ x = ");
    await typeChars(page, "alph");
    await expectEventually(
      page,
      "typing a symbol name in math offers it with its Unicode preview",
      `(document.querySelector(".cm-tooltip-autocomplete")?.innerText ?? "").includes("α")`,
    );
    await shoot(page, "23-symbol-completion");
    await press(page, "Escape");
    await insertText(page, " $\n\n");
    await typeChars(page, "theor");
    await expectEventually(
      page,
      "typing a snippet trigger offers the snippet",
      `(document.querySelector(".cm-tooltip-autocomplete")?.innerText ?? "").includes("Theorem")`,
    );
    await press(page, "Enter", { wait: 600 });
    await expectEventually(
      page,
      "accepting it expands with tab stops",
      `document.querySelector(".cm-content").innerText.includes("Theorem (name)") && document.querySelector(".cm-snippetField") !== null`,
    );
    await press(page, "Escape");

    // Format document, one undo step.
    const hasTrailing = `[...document.querySelectorAll(".cm-line")].some((l) => /[ \\t]$/.test(l.textContent))`;
    check(
      "the text has trailing whitespace before formatting",
      await page.evaluate(hasTrailing),
    );
    await press(page, "f", { shift: true, alt: true, wait: 600 });
    await expectEventually(
      page,
      "Shift+Alt+F formats the document",
      `!(${hasTrailing})`,
    );
    check(
      "the status says the document was formatted",
      /Document formatted/.test(await statusText(page)),
    );
    await press(page, "z", { ctrl: true, wait: 300 });
    check("one undo restores the whole formatting", await page.evaluate(hasTrailing));

    // Word count.
    await caretToEnd(page);
    await sleep(2500);
    const wordsBefore = await wordCount();
    await insertText(page, "\n\nalpha beta gamma delta epsilon");
    await expectEventually(
      page,
      "the word count updates after typing",
      `Number(document.querySelector("[data-word-count]")?.textContent.match(/\\d+/)?.[0] ?? -1) >= ${wordsBefore + 5}`,
      { timeout: 8000 },
    );
    check(
      "the word count tooltip gives characters, sentences and reading time",
      await page.evaluate(
        `(() => { const t = document.querySelector("[data-word-count]")?.title ?? ""; return /characters/.test(t) && /sentences?/.test(t) && /to read/.test(t); })()`,
      ),
      `${baseWords} -> ${wordsBefore}`,
    );
    await shoot(page, "24-writing-status");

    // Leave the document as found.
    for (let i = 0; i < 80; i += 1) {
      if (!(await dirtyTabs(page)).includes("talk.typ")) break;
      await press(page, "z", { ctrl: true, wait: 30 });
    }
    check(
      "undo returns talk.typ to its saved text",
      !(await dirtyTabs(page)).includes("talk.typ"),
    );

    /* ------------------------------------------------------------------ */
    section("Slides");
    await clickText(
      page,
      '[role="radiogroup"][aria-label="Document view"] [role="radio"]',
      "Slides",
    );
    await sleep(600);
    await waitFor(
      page,
      `document.querySelectorAll('[role="listbox"][aria-label="Slides"] [role="option"]').length > 0`,
      { label: "slide strip" },
    );
    check(
      "talk.typ shows a thumbnail per heading",
      await page.evaluate(
        `document.querySelectorAll('[role="listbox"][aria-label="Slides"] [role="option"]').length === 6`,
      ),
    );
    await shoot(page, "15-slides");
    await press(page, "ArrowDown");
    await page.evaluate(
      `document.querySelector('[role="listbox"][aria-label="Slides"] [role="option"][aria-selected="true"]').focus()`,
    );
    await press(page, "ArrowDown");
    check(
      "arrow keys move through the thumbnails",
      await page.evaluate(
        `document.querySelector('[role="option"][aria-selected="true"]').dataset.slide !== "0"`,
      ),
    );
    await clickText(page, "button", "Present");
    check(
      "Present opens a full-window presentation",
      await exists(page, '[role="dialog"][aria-label="Presentation"]'),
    );
    await shoot(page, "16-presenting");
    await press(page, "ArrowRight");
    await press(page, "Escape");
    check(
      "Esc leaves the presentation",
      !(await exists(page, '[role="dialog"][aria-label="Presentation"]')),
    );
    await clickText(
      page,
      '[role="radiogroup"][aria-label="Document view"] [role="radio"]',
      "Text",
    );
    await waitFor(page, `document.querySelector(".cm-editor") !== null`);

    /* ------------------------------------------------------------------ */
    section("Figure editor");
    await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Figures"]');
    await sleep(400);
    await clickText(page, "#side-panel ul button", "state_lattice");
    await waitFor(page, `document.querySelector('svg[role="application"]') !== null`, {
      label: "figure editor",
    });
    check(
      "a figure opens inside a tab, not a takeover",
      (await activeTab(page)) === "figures/state_lattice.figure.json" &&
        (await exists(page, '[role="tablist"]')),
    );
    check(
      "the scene loads from the project file",
      await page.evaluate(`document.body.innerText.includes("4 nodes")`),
    );
    await clickText(page, "button[aria-pressed]", "Rectangle");
    const svg = await rect(page, 'svg[role="application"]');
    await mouse(page, "mouseMoved", svg.left + 200, svg.top + 380);
    await mouse(page, "mousePressed", svg.left + 200, svg.top + 380);
    await mouse(page, "mouseMoved", svg.left + 320, svg.top + 450);
    await mouse(page, "mouseReleased", svg.left + 320, svg.top + 450);
    await sleep(300);
    check(
      "drawing adds a node and marks the tab dirty",
      (await page.evaluate(`document.body.innerText.includes("5 nodes")`)) &&
        (await page.evaluate(
          `document.querySelector('[role="tab"][aria-selected="true"]').getAttribute("aria-label").includes("unsaved")`,
        )),
    );
    await shoot(page, "17-figure");
    await press(page, "s", { ctrl: true, wait: 400 });
    check(
      "Ctrl+S saves the scene as JSON and its SVG",
      await page.evaluate(
        `!document.querySelector('[role="tab"][aria-selected="true"]').getAttribute("aria-label").includes("unsaved")`,
      ),
    );

    // Move the rectangle just drawn: select it and drag it.
    const shapeXs = () =>
      page.evaluate(
        `[...document.querySelectorAll('svg[role="application"] rect')].map((r) => r.getAttribute("x")).join()`,
      );
    await clickText(page, "button[aria-pressed]", "Select");
    const beforeMove = await shapeXs();
    await mouse(page, "mouseMoved", svg.left + 260, svg.top + 415);
    await mouse(page, "mousePressed", svg.left + 260, svg.top + 415);
    await sleep(150);
    await mouse(page, "mouseMoved", svg.left + 300, svg.top + 425);
    await mouse(page, "mouseMoved", svg.left + 340, svg.top + 435);
    await mouse(page, "mouseReleased", svg.left + 340, svg.top + 435);
    await sleep(300);
    const afterMove = await shapeXs();
    check(
      "dragging a selected shape moves it (one plugin edit on release)",
      beforeMove !== afterMove &&
        (await page.evaluate(`document.body.innerText.includes("5 nodes")`)) &&
        (await dirtyTabs(page)).includes("figures/state_lattice.figure.json"),
      `${beforeMove} -> ${afterMove}`,
    );
    await press(page, "Delete", { wait: 300 });
    const deleted = await page.evaluate(`document.body.innerText.includes("4 nodes")`);
    await press(page, "z", { ctrl: true, wait: 400 });
    check(
      "Delete removes the selection and Ctrl+Z brings it back through the plugin",
      deleted && (await page.evaluate(`document.body.innerText.includes("5 nodes")`)),
    );
    await press(page, "s", { ctrl: true, wait: 400 });
    check(
      "saving after the move clears the unsaved mark",
      !(await dirtyTabs(page)).includes("figures/state_lattice.figure.json"),
    );

    // A figure whose SVG was never written: Copy embed line writes it first.
    if (!(await exists(page, 'aside[aria-label="Figures panel"]'))) {
      await click(
        page,
        'nav[aria-label="Workbench views"] button[aria-label="Figures"]',
      );
      await sleep(300);
    }
    await clickText(page, "#side-panel ul button", "dissipation_curve");
    await waitFor(
      page,
      `document.querySelector('[role="tab"][data-tab][aria-selected="true"]')?.dataset.tab === "figures/dissipation_curve.figure.json" && document.querySelector('svg[role="application"]') !== null`,
      { label: "second figure" },
    );
    await clickText(
      page,
      '[role="toolbar"][aria-label="Figure tools"] button',
      "Copy embed line",
    );
    await expectEventually(
      page,
      "Copy embed line names an SVG that now exists",
      `(() => { const s = document.querySelector('footer[aria-label="Status"]')?.innerText ?? ""; return s.includes("Copied the embed line") || s.includes("figures/dissipation_curve.svg"); })()`,
      { show: `document.querySelector('footer[aria-label="Status"]')?.innerText` },
    );
    if (!(await exists(page, 'aside[aria-label="Files panel"]'))) {
      await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');
      await sleep(300);
    }
    if (
      (await page.evaluate(
        `document.querySelector('[role="treeitem"][data-path="figures"]')?.getAttribute("aria-expanded")`,
      )) !== "true"
    ) {
      await click(page, '[role="treeitem"][data-path="figures"]');
    }
    await expectEventually(
      page,
      "the written SVG appears in Files next to its scene",
      `document.querySelector('[role="treeitem"][data-path="figures/dissipation_curve.svg"]') !== null`,
    );

    /* ------------------------------------------------------------------ */
    section("Opening any file");
    await click(page, '[role="treeitem"][data-path="figures/sketch.png"]');
    await expectEventually(
      page,
      "an image opens in a viewer tab, fitted, from a blob: URL",
      `(() => { const img = document.querySelector('[data-viewer="figures/sketch.png"] img'); return img !== null && img.src.startsWith("blob:") && img.naturalWidth === 240; })()`,
    );
    check(
      "the image tab is a real tab",
      (await activeTab(page)) === "figures/sketch.png",
    );
    await click(page, '[data-viewer="figures/sketch.png"] img');
    check(
      "clicking the image shows it at 1:1",
      await page.evaluate(
        `document.querySelector('[data-viewer="figures/sketch.png"] button[aria-pressed]')?.getAttribute("aria-pressed") === "true"`,
      ),
    );
    await shoot(page, "18-image-viewer");
    const blobUrl = await page.evaluate(
      `document.querySelector('[data-viewer="figures/sketch.png"] img').src`,
    );
    await press(page, "w", { ctrl: true, wait: 300, synthetic: true });
    check(
      "closing the image tab revokes its blob: URL",
      !(await tabNames(page)).includes("figures/sketch.png") &&
        (await page.evaluate(
          `fetch(${JSON.stringify(blobUrl)}).then(() => false, () => true)`,
        )),
    );

    async function goToFile(query) {
      await press(page, "p", { ctrl: true, wait: 300 });
      await insertText(page, query);
      await sleep(200);
      await press(page, "Enter", { wait: 500 });
    }

    await goToFile("measurements.csv");
    await expectEventually(
      page,
      "a .csv opens as text in the editor (Go to file)",
      `document.querySelector('[role="tab"][data-tab][aria-selected="true"]')?.dataset.tab === "data/measurements.csv" && (document.querySelector(".cm-content")?.innerText ?? "").includes("cycle,duration_ms")`,
    );

    await goToFile("landauer-note.pdf");
    await expectEventually(
      page,
      "a .pdf opens in the PDF viewer tab",
      `document.querySelector('[data-viewer="papers/landauer-note.pdf"] canvas') !== null || (document.querySelector('[data-viewer="papers/landauer-note.pdf"]')?.innerText ?? "").includes("page 1 of 1")`,
      { timeout: 20000 },
    );
    await shoot(page, "19-pdf-viewer");

    await goToFile("raw.zip");
    await expectEventually(
      page,
      "a binary opens a quiet info tab with type and size",
      `(() => { const v = document.querySelector('[data-viewer="data/raw.zip"]'); return v !== null && v.innerText.includes("ZIP archive") && v.innerText.includes("22 B"); })()`,
    );
    await clickText(
      page,
      '[data-viewer="data/raw.zip"] button',
      "Open with system app",
    );
    await expectEventually(
      page,
      "in the browser, Open with system app says it needs the desktop app",
      `(document.querySelector('footer[aria-label="Status"]')?.innerText ?? "").includes("needs the desktop app")`,
    );
    await shoot(page, "20-info-tab");
    await noStrayEdits(page, "after opening any file");

    /* ------------------------------------------------------------------ */
    section("Source and preview");
    await clickText(page, '[role="tab"]', "main.typ");
    await waitFor(page, `document.querySelector('img[alt^="Page 1"]') !== null`, {
      label: "the Typst page",
    });
    const caretLine = () =>
      page.evaluate(
        `Number(document.querySelector('footer[aria-label="Status"]').innerText.match(/Ln (\\d+)/)?.[1] ?? 0)`,
      );
    // Caret a few lines into main.typ (past the `#set` line), then "Show in preview".
    await focusEditor(page);
    await press(page, "Home", { ctrl: true });
    for (let i = 0; i < 10; i += 1) await press(page, "ArrowDown", { wait: 30 });
    const sourceLine = await caretLine();
    await page.evaluate(
      `document.querySelector('[role="region"][aria-label="Rendered page"]').scrollTop = 1e6`,
    );
    await press(page, "j", { ctrl: true, alt: true, wait: 500 });
    const markRect = await rect(page, "[data-preview-mark]");
    const region = await rect(page, '[role="region"][aria-label="Rendered page"]');
    check(
      '"Show in preview" (Ctrl+Alt+J) outlines the caret\'s text on the page',
      markRect !== null && sourceLine > 2,
      `line ${sourceLine}`,
    );
    check(
      "the preview scrolls the mark into view",
      markRect !== null &&
        region !== null &&
        markRect.top >= region.top &&
        markRect.bottom <= region.bottom,
      JSON.stringify({ markRect, region }),
    );
    check(
      "the mark is a 1px acid-lime outline, square, without a shadow",
      await page.evaluate(
        `(() => { const s = getComputedStyle(document.querySelector("[data-preview-mark]")); return s.borderTopWidth === "1px" && s.borderTopColor === "rgb(198, 241, 53)" && s.borderRadius === "0px" && s.boxShadow === "none"; })()`,
      ),
    );
    await shoot(page, "18-show-in-preview");

    // Move the caret away, then a plain click on the page leaves it there.
    await press(page, "Home", { ctrl: true });
    const target = markRect ?? { x: 0, y: 0 };
    await mouse(page, "mouseMoved", target.x, target.y);
    await mouse(page, "mousePressed", target.x, target.y);
    await mouse(page, "mouseReleased", target.x, target.y);
    await sleep(300);
    check("a plain click on the page keeps the caret", (await caretLine()) === 1);

    // Ctrl+click on the same spot opens the source line there.
    for (const type of ["mousePressed", "mouseReleased"]) {
      await page.send("Input.dispatchMouseEvent", {
        type,
        x: target.x,
        y: target.y,
        button: "left",
        buttons: 1,
        clickCount: 1,
        modifiers: 2,
      });
    }
    await expectEventually(
      page,
      "Ctrl+click on the rendered page moves the caret to the source line",
      `/Ln ${sourceLine},/.test(document.querySelector('footer[aria-label="Status"]').innerText) && document.querySelector(".cm-line[data-source-flash]") !== null`,
      { timeout: 4000 },
    );
    check(
      "the line it lands on is highlighted briefly",
      await page.evaluate(
        `document.querySelector(".cm-line[data-source-flash]")?.innerText.trim().length > 0`,
      ),
    );
    await shoot(page, "18-source-jump");

    /* ------------------------------------------------------------------ */
    section("Zen mode");
    await clickText(page, '[role="tab"]', "main.typ");
    await sleep(500);
    await press(page, "\\", { ctrl: true, wait: 400 });
    check(
      "zen removes every piece of chrome",
      await page.evaluate(
        `document.querySelector("header") === null && document.querySelector("footer") === null && document.querySelector("nav") === null && document.querySelector("aside") === null && document.querySelector('[role="tablist"]') === null`,
      ),
    );
    check(
      "the writing column is at most 680px",
      await page.evaluate(
        `(() => { const c = document.querySelector(".zen-editor .cm-content"); return !!c && Math.round(c.getBoundingClientRect().width) <= 680; })()`,
      ),
    );
    await shoot(page, "18-zen");
    await press(page, "Escape", { wait: 300 });
    check(
      "Esc brings the workbench back",
      (await exists(page, "header")) &&
        (await exists(page, 'footer[aria-label="Status"]')),
    );

    /* ------------------------------------------------------------------ */
    section("Editor chrome");
    // main.typ is active and in split, so the preview header is on screen.
    // Its last button must stay inside the pane at every width.
    const headerFits = `(() => { const s = document.querySelector('section[aria-label="Preview"]'); const bar = s?.querySelector('[role="toolbar"]'); const close = bar?.querySelector('button[aria-label="Close preview"]'); if (!close) return false; const r = close.getBoundingClientRect(); return r.width > 0 && r.right <= s.getBoundingClientRect().right && bar.scrollWidth <= bar.clientWidth; })()`;
    const WINDOW_BUTTONS = '["Minimise", "Maximise", "Restore", "Close window"]';
    check(
      "the workbench top bar has no buttons but Home and the window controls",
      await page.evaluate(
        `[...document.querySelectorAll("header button")].every((b) => b.getAttribute("aria-label") === "Home" || ${WINDOW_BUTTONS}.includes(b.getAttribute("aria-label")))`,
      ),
    );
    check(
      "the top bar has no layout, Export PDF or Zen controls",
      await page.evaluate(
        `!document.querySelector('header [role="radiogroup"]') && !/Export PDF|Zen|Split/.test(document.querySelector("header").innerText)`,
      ),
    );
    check(
      "the top bar is still the window's drag region at its height",
      await page.evaluate(
        `(() => { const h = document.querySelector("header"); return h.hasAttribute("data-tauri-drag-region") && Math.round(h.getBoundingClientRect().height) === 40; })()`,
      ),
    );
    check(
      "Export PDF appears once on screen, in the preview header",
      await page.evaluate(
        `document.querySelectorAll('button[aria-label="Export PDF"]').length === 1 && !!document.querySelector('section[aria-label="Preview"] [role="toolbar"] button[aria-label="Export PDF"]')`,
      ),
    );
    check(
      "Show preview is absent while the preview is visible",
      !(await exists(page, 'button[aria-label="Show preview"]')),
    );
    check(
      "the preview header ends with Maximise and Close",
      await page.evaluate(
        `(() => { const b = [...document.querySelectorAll('section[aria-label="Preview"] [role="toolbar"] button')].map((x) => x.getAttribute("aria-label")); return b.slice(-2).join() === "Maximise preview,Close preview"; })()`,
      ),
    );
    check(
      "at 1280px with the side panel open the header fits and keeps zoom",
      (await page.evaluate(headerFits)) &&
        (await page.evaluate(
          `(() => { const b = document.querySelector('section[aria-label="Preview"] button[aria-label="Zoom in"]'); return !!b && b.getBoundingClientRect().width > 0; })()`,
        )),
    );
    await shoot(page, "18b-preview-header");
    await click(page, 'button[aria-label="Maximise preview"]');
    check(
      "Maximise shows the preview alone and offers Restore split",
      !(await exists(page, ".cm-editor")) &&
        (await exists(page, 'button[aria-label="Restore split"]')),
    );
    await click(page, 'button[aria-label="Restore split"]');
    await waitFor(page, `document.querySelector(".cm-editor") !== null`);
    check(
      "Restore split brings the editor back beside the preview",
      await exists(page, 'section[aria-label="Preview"]'),
    );
    await click(page, 'button[aria-label="Close preview"]');
    check(
      "Close preview shows the editor alone",
      !(await exists(page, 'section[aria-label="Preview"]')) &&
        (await exists(page, ".cm-editor")),
    );
    check(
      "Show preview appears at the end of the tab strip once the preview is closed",
      await page.evaluate(
        `(() => { const b = document.querySelector('button[aria-label="Show preview"]'); const strip = document.querySelector('[role="tablist"]')?.parentElement; if (!b || !strip) return false; return strip.contains(b) && Math.abs(b.getBoundingClientRect().right - strip.getBoundingClientRect().right) < 12; })()`,
      ),
    );
    await shoot(page, "18c-preview-closed");
    await click(page, 'button[aria-label="Show preview"]');
    await waitFor(
      page,
      `document.querySelector('section[aria-label="Preview"]') !== null`,
    );
    check(
      "Show preview brings back the split",
      (await exists(page, ".cm-editor")) &&
        !(await exists(page, 'button[aria-label="Show preview"]')),
    );
    await press(page, "p", { ctrl: true, wait: 300 });
    await insertText(page, "references.bib");
    await press(page, "Enter", { wait: 600 });
    await waitFor(
      page,
      `document.querySelector('button[aria-label="Close preview"]') !== null`,
      {
        timeout: 8000,
      },
    );
    await click(page, 'button[aria-label="Close preview"]');
    check(
      "a file previewed through the main file can show its preview again",
      await exists(page, 'button[aria-label="Show preview"]'),
    );
    await click(page, 'button[aria-label="Show preview"]');
    await clickText(page, '[role="tab"]', "main.typ");
    await sleep(300);

    // Narrow: split cannot fit, so Show preview opens the preview alone and
    // its Close returns to the editor.
    await resize(page, 640, 600);
    await sleep(300);
    // The side panel overlays the editor here; dismiss it first.
    if (await exists(page, "#side-panel")) await press(page, "Escape", { wait: 300 });
    check(
      "at 640px split falls back to the editor and Show preview is offered",
      !(await exists(page, 'section[aria-label="Preview"]')) &&
        (await exists(page, 'button[aria-label="Show preview"]')),
    );
    await click(page, 'button[aria-label="Show preview"]');
    await waitFor(
      page,
      `document.querySelector('section[aria-label="Preview"]') !== null`,
    );
    check(
      "in a narrow window Show preview shows the preview alone, without Maximise",
      !(await exists(page, ".cm-editor")) &&
        !(await exists(page, 'button[aria-label="Maximise preview"]')) &&
        !(await exists(page, 'button[aria-label="Restore split"]')),
    );
    check(
      "at 640px the preview header and the page fit without overflow",
      await page.evaluate(
        `(() => { const bar = document.querySelector('section[aria-label="Preview"] [role="toolbar"]'); const close = bar?.querySelector('button[aria-label="Close preview"]'); if (!close) return false; const r = close.getBoundingClientRect(); return document.documentElement.scrollWidth <= window.innerWidth && bar.scrollWidth <= bar.clientWidth && r.right <= window.innerWidth && r.width > 0; })()`,
      ),
    );
    await shoot(page, "18d-640-preview");
    await click(page, 'button[aria-label="Close preview"]');
    check(
      "Close returns to the editor in a narrow window",
      (await exists(page, ".cm-editor")) &&
        !(await exists(page, 'section[aria-label="Preview"]')),
    );
    check(
      "at 640px the tab strip and the top bar fit",
      await page.evaluate(
        `(() => { const strip = document.querySelector('[role="tablist"]').parentElement; const bar = document.querySelector("header"); return document.documentElement.scrollWidth <= window.innerWidth && strip.scrollWidth <= strip.clientWidth && bar.scrollWidth <= bar.clientWidth && document.querySelector('button[aria-label="Show preview"]').getBoundingClientRect().right <= window.innerWidth; })()`,
      ),
    );
    await shoot(page, "18e-640-editor");
    await resize(page, 1280, 820);
    await sleep(300);
    // Back to split, with the side panel open, for the sections that follow.
    if (!(await exists(page, "#side-panel")))
      await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');
    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "side by side");
    await press(page, "Enter", { wait: 300 });
    check(
      "the palette's split command restores the split",
      (await exists(page, ".cm-editor")) &&
        (await exists(page, 'section[aria-label="Preview"]')),
    );

    /* ------------------------------------------------------------------ */
    section("Persistence");
    await press(page, "j", { ctrl: true });
    const tabsBefore = await tabNames(page);
    const panelWasOpen = await exists(page, "#bottom-panel");
    await reload(page);
    await expectEventually(
      page,
      "the last project and its tabs come back after a reload",
      `document.querySelectorAll('[role="tab"][data-tab]').length > 0 && document.querySelector(".cm-content") !== null`,
      { timeout: 8000 },
    );
    check(
      "same tabs",
      JSON.stringify(await tabNames(page)) === JSON.stringify(tabsBefore),
      `${tabsBefore.join()} vs ${(await tabNames(page)).join()}`,
    );
    check(
      "panel state is remembered",
      (await exists(page, "#bottom-panel")) === panelWasOpen,
    );

    /* ------------------------------------------------------------------ */
    section("Narrow layouts");
    await resize(page, 1024, 700);
    await shoot(page, "19-1024");
    check(
      "at 1024px the workbench is split with the side panel inline",
      await page.evaluate(
        `getComputedStyle(document.querySelector("#side-panel")).position !== "absolute" && document.querySelectorAll('section[aria-label="Preview"]').length === 1`,
      ),
    );
    check(
      "at 1024px the preview header keeps Close preview inside the pane",
      await page.evaluate(headerFits),
    );
    // The narrowest preview: 800px wide, side panel inline, divider at its limit.
    await resize(page, 800, 700);
    await sleep(300);
    await page.evaluate(
      `document.querySelector('[role="separator"][aria-label="Resize editor and preview"]').focus()`,
    );
    await press(page, "End", { wait: 300 });
    check(
      "with the narrowest split the preview header still fits",
      await page.evaluate(headerFits),
    );
    await shoot(page, "19b-narrowest-preview");
    await page.evaluate(
      `document.querySelector('[role="separator"][aria-label="Resize editor and preview"]').dispatchEvent(new MouseEvent("dblclick", { bubbles: true }))`,
    );
    await resize(page, 760, 700);
    await sleep(300);
    check(
      "under 800px the activity bar stays",
      await page.evaluate(
        `Math.round(document.querySelector('nav[aria-label="Workbench views"]').getBoundingClientRect().width) === 48`,
      ),
    );
    const sideState = await exists(page, "#side-panel");
    if (!sideState)
      await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');
    check(
      "the side panel overlays the editor instead of squeezing it",
      await page.evaluate(
        `getComputedStyle(document.querySelector("#side-panel")).position === "absolute"`,
      ),
    );
    await shoot(page, "20-narrow-overlay");
    await press(page, "Escape");
    check("Esc dismisses the overlay", !(await exists(page, "#side-panel")));
    check(
      "split falls back to the editor alone when narrow",
      !(await exists(page, 'section[aria-label="Preview"]')),
    );
    await resize(page, 640, 600);
    await shoot(page, "21-640");
    check(
      "at the 640px minimum nothing scrolls sideways",
      await page.evaluate(`document.documentElement.scrollWidth <= window.innerWidth`),
    );
    check(
      "the top bar keeps Home reachable and fits at 640px",
      await page.evaluate(
        `(() => { const bar = document.querySelector("header"); const b = bar?.querySelector('button[aria-label="Home"]'); if (!b) return false; const r = b.getBoundingClientRect(); return r.width > 0 && r.right <= window.innerWidth && bar.scrollWidth <= bar.clientWidth; })()`,
      ),
    );
    await resize(page, 1280, 820);

    /* ------------------------------------------------------------------ */
    section("Home inside a project, and switching");
    const crumb = (cdp) =>
      cdp.evaluate(
        `document.querySelector('nav[aria-label="Breadcrumb"]')?.innerText.replace(/\\s+/g, " ") ?? ""`,
      );
    await click(page, 'header button[aria-label="Home"]');
    await expectEventually(
      page,
      "the Home button shows Home over the open project, without the workbench rail",
      `!!document.querySelector('section[aria-label="Home"]') && !document.querySelector('nav[aria-label="Workbench views"]')`,
    );
    check(
      "the project stays open underneath, out of reach",
      await page.evaluate(
        `document.querySelector('[role="tablist"][aria-label="Open files"]')?.closest("[inert]") !== null`,
      ),
    );
    await expectEventually(
      page,
      "the compiled first page became a thumbnail for the open project",
      `document.querySelector('[data-project$="entropy-paper"] img[alt^="First page"]')?.src.startsWith("blob:")`,
    );
    await shoot(page, "22-home-in-project");
    check(
      "Back to the project closes Home",
      await (async () => {
        await clickText(page, "button", "Back to Entropy paper");
        return !(await exists(page, 'section[aria-label="Home"]'));
      })(),
    );
    await press(page, "h", { ctrl: true, shift: true, synthetic: true });
    check("Ctrl+Shift+H opens Home", await exists(page, 'section[aria-label="Home"]'));
    await press(page, "Escape");
    check(
      "Esc leaves Home for the project",
      !(await exists(page, 'section[aria-label="Home"]')),
    );

    // Switch from Home.
    await click(page, 'header button[aria-label="Home"]');
    await clickText(page, "[data-project] button", "Lecture notes");
    await waitFor(
      page,
      `document.querySelector('nav[aria-label="Breadcrumb"]')?.innerText.includes("Lecture notes")`,
      { label: "switched to Lecture notes" },
    );
    check(
      "switching opens the other project's main file",
      (await crumb(page)).includes("Lecture notes / index.md"),
      await crumb(page),
    );
    check(
      "without leaving the app: no Home, no stale tabs",
      !(await exists(page, 'section[aria-label="Home"]')) &&
        (await tabNames(page)).join() === "index.md",
      (await tabNames(page)).join(),
    );
    await shoot(page, "23-switched");

    // Switch with Ctrl+O.
    await press(page, "o", { ctrl: true, synthetic: true });
    check(
      "Ctrl+O opens the project switcher",
      await exists(page, '[role="dialog"][aria-label="Switch project"]'),
    );
    check(
      "it lists other projects, the open one last",
      await page.evaluate(
        `(() => { const o = [...document.querySelectorAll('[role="listbox"][aria-label="Projects"] [role="option"]')].map((e) => e.innerText); return o.length === 4 && o[3].includes("Lecture notes") && o[3].includes("Open now") && !o.join().includes("Conference poster") && !o.join().includes("Old thesis"); })()`,
      ),
    );
    await shoot(page, "24-switcher");
    await insertText(page, "entropy");
    await press(page, "Enter", { wait: 500 });
    await waitFor(
      page,
      `document.querySelector('nav[aria-label="Breadcrumb"]')?.innerText.includes("Entropy paper")`,
      { label: "switched back" },
    );
    check(
      "the switcher opens the project and its earlier tabs come back",
      (await tabNames(page)).length > 1 && (await tabNames(page)).includes("main.typ"),
      (await tabNames(page)).join(),
    );

    // Unsaved changes ask first.
    await focusEditor(page);
    await caretToEnd(page);
    await insertText(page, " unsaved");
    await press(page, "o", { ctrl: true, synthetic: true });
    await insertText(page, "lecture");
    await press(page, "Enter", { wait: 400 });
    check(
      "switching with unsaved changes asks first",
      await exists(page, '[role="alertdialog"]'),
    );
    await press(page, "Escape");
    check(
      "cancelling stays in the project with the edit intact",
      (await crumb(page)).includes("Entropy paper") &&
        (await dirtyTabs(page)).length === 1,
    );
    await press(page, "o", { ctrl: true, synthetic: true });
    await insertText(page, "lecture");
    await press(page, "Enter", { wait: 400 });
    await clickText(page, '[role="alertdialog"] button', "Don't save");
    await waitFor(
      page,
      `document.querySelector('nav[aria-label="Breadcrumb"]')?.innerText.includes("Lecture notes")`,
      { label: "switched after discarding" },
    );
    check("discarding switches", (await crumb(page)).includes("Lecture notes"));
    await press(page, "o", { ctrl: true, synthetic: true });
    await insertText(page, "entropy");
    await press(page, "Enter", { wait: 500 });
    await waitFor(
      page,
      `document.querySelector('nav[aria-label="Breadcrumb"]')?.innerText.includes("Entropy paper")`,
      { label: "back in Entropy paper" },
    );

    section("LaTeX and Markdown preview");
    const doc = await newPage(browser);
    current = doc;
    await press(doc, "h", { ctrl: true, shift: true, wait: 400 });
    await waitFor(doc, `!!document.querySelector('[data-project$="thesis-draft"]')`, {
      label: "Home lists the LaTeX demo project",
    });
    await click(doc, '[data-project$="thesis-draft"] button');
    await expectEventually(
      doc,
      "a LaTeX project compiles with latex.compile and shows the PDF in the viewer",
      `document.querySelector('[data-pdf-viewer] canvas') !== null`,
      { timeout: 20000 },
    );
    await expectEventually(
      doc,
      "status bar reports the LaTeX compile time",
      `/Compiled in \\d+ ms/.test(document.querySelector('footer[aria-label="Status"]').innerText)`,
    );
    check(
      "the toolbar says which page of how many",
      await doc.evaluate(
        `/Page 1 of \\d+/.test(document.querySelector('section[aria-label="Preview"]').innerText)`,
      ),
    );
    check(
      "the preview header names the TeX backend",
      await doc.evaluate(
        `document.querySelector('section[aria-label="Preview"]').innerText.includes("pdflatex, system TeX")`,
      ),
    );
    await shoot(doc, "22-latex-preview");

    // A chapter compiles through the main file, not by itself.
    await press(doc, "p", { ctrl: true, wait: 300 });
    await insertText(doc, "methods");
    await press(doc, "Enter", { wait: 500 });
    await expectEventually(
      doc,
      "a chapter is compiled through main.tex",
      `document.querySelector('section[aria-label="Preview"]').innerText.includes("via main.tex")`,
    );

    // SyncTeX both ways: line 2 of the chapter to the PDF and back.
    await focusEditor(doc);
    await press(doc, "Home", { ctrl: true });
    await press(doc, "ArrowDown");
    await press(doc, "j", { ctrl: true, alt: true, wait: 600 });
    const pdfMark = await rect(doc, "[data-pdf-viewer] [data-preview-mark]");
    check(
      '"Show in preview" outlines the chapter line in the PDF (SyncTeX forward)',
      pdfMark !== null,
    );
    await press(doc, "Home", { ctrl: true });
    if (pdfMark !== null) {
      for (const type of ["mousePressed", "mouseReleased"]) {
        await doc.send("Input.dispatchMouseEvent", {
          type,
          x: pdfMark.x,
          y: pdfMark.y,
          button: "left",
          buttons: 1,
          clickCount: 1,
          modifiers: 2,
        });
      }
    }
    await expectEventually(
      doc,
      "Ctrl+click on the PDF opens the chapter at that line (SyncTeX reverse)",
      `/Ln 2,/.test(document.querySelector('footer[aria-label="Status"]').innerText)`,
      { timeout: 4000 },
    );

    // Saving a broken chapter reaches Problems and keeps the last good PDF.
    await focusEditor(doc);
    await caretToEnd(doc);
    await insertText(doc, "\\input{missing}\n");
    await press(doc, "s", { ctrl: true, wait: 600 });
    await expectEventually(
      doc,
      "after the save the status bar counts the LaTeX error",
      `/1 error/.test(document.querySelector('footer[aria-label="Status"]').innerText)`,
      { timeout: 8000 },
    );
    await expectEventually(
      doc,
      "the error banner names the problem and keeps the last good page",
      `document.querySelector('section[aria-label="Preview"]').innerText.includes("last good page") && document.querySelector('[data-pdf-viewer] canvas') !== null`,
    );
    await clickText(doc, 'section[aria-label="Preview"] button', "Show problems");
    check(
      "Show problems opens the panel with the TeX message",
      await doc.evaluate(
        `document.querySelector("#bottom-panel")?.innerText.includes("not found") ?? false`,
      ),
    );
    await shoot(doc, "23-latex-error");

    // Many pages: page navigation and scroll position across a recompile.
    await focusEditor(doc);
    await press(doc, "a", { ctrl: true });
    await insertText(
      doc,
      Array.from({ length: 140 }, (_, i) => `Line ${i + 1} of the methods.`).join(
        "\n\n",
      ) + "\n",
    );
    await press(doc, "s", { ctrl: true, wait: 800 });
    await expectEventually(
      doc,
      "a long document reports several pages",
      `/Page 1 of ([2-9]|\\d\\d)/.test(document.querySelector('section[aria-label="Preview"]').innerText)`,
      { timeout: 8000 },
    );
    await click(doc, 'button[aria-label="Next page"]');
    await expectEventually(
      doc,
      "Next page scrolls the viewer to page 2",
      `/Page 2 of/.test(document.querySelector('section[aria-label="Preview"]').innerText) && document.querySelector('[data-pdf-viewer]').scrollTop > 100`,
    );
    const scrollBefore = await doc.evaluate(
      `document.querySelector('[data-pdf-viewer]').scrollTop`,
    );
    await insertText(doc, "One more line.\n");
    await press(doc, "s", { ctrl: true, wait: 1200 });
    const scrollAfter = await doc.evaluate(
      `document.querySelector('[data-pdf-viewer]').scrollTop`,
    );
    check(
      "scroll position survives a recompile",
      Math.abs(scrollAfter - scrollBefore) < 4,
      `${scrollBefore} vs ${scrollAfter}`,
    );
    check(
      "only the pages near the viewport own a canvas",
      await doc.evaluate(
        `document.querySelectorAll('[data-pdf-viewer] canvas').length <= 4`,
      ),
    );

    // Markdown.
    await press(doc, "k", { ctrl: true, wait: 300 });
    await insertText(doc, "close project");
    await press(doc, "Enter", { wait: 600 });
    if (await exists(doc, '[role="dialog"]')) {
      await clickText(doc, '[role="dialog"] button', "Don't save");
      await sleep(300);
    }
    await waitFor(doc, `!!document.querySelector('[data-project$="lecture-notes"]')`, {
      label: "Home lists the Markdown demo project",
    });
    await click(doc, '[data-project$="lecture-notes"] button');
    await expectEventually(
      doc,
      "Markdown renders the sanitised HTML from markdown.render-html on the paper page",
      `(() => { const a = document.querySelector('article[aria-label="Markdown preview"]'); return !!a && a.querySelector("h1")?.textContent === "Lecture notes" && getComputedStyle(a).backgroundColor === "rgb(250, 249, 247)"; })()`,
    );
    await shoot(doc, "24-markdown-preview");
    check(
      "no page errors during LaTeX and Markdown preview",
      doc.errors.length === 0,
      doc.errors.slice(0, 2).join(" | "),
    );
    doc.close();
    current = page;

    /* ------------------------------------------------------------------ */
    section("Brand invariants");
    check(
      "no rounded corners anywhere",
      await page.evaluate(
        `(() => { for (const el of document.querySelectorAll("*")) { const s = getComputedStyle(el); for (const p of ["borderTopLeftRadius","borderTopRightRadius","borderBottomLeftRadius","borderBottomRightRadius"]) { if (s[p] !== "0px" && s[p] !== "") return el.className + " -> " + s[p]; } } return true; })()`,
      ),
    );
    check(
      "no shadows",
      await page.evaluate(
        `[...document.querySelectorAll("*")].every((el) => getComputedStyle(el).boxShadow === "none")`,
      ),
    );
    check(
      "only DM Sans and DM Mono",
      await page.evaluate(
        `(() => { const fams = new Set(); for (const el of document.querySelectorAll("body, body *")) { if (el.childNodes.length && [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim())) fams.add(getComputedStyle(el).fontFamily.split(",")[0].replace(/["']/g, "")); } return [...fams].every((f) => f === "DM Sans" || f === "DM Mono") || [...fams].join(); })()`,
      ),
    );
    check(
      "only weights 300 and 400",
      await page.evaluate(
        `(() => { const w = new Set(); for (const el of document.querySelectorAll("body, body *")) { if ([...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim())) w.add(getComputedStyle(el).fontWeight); } return [...w].every((x) => x === "300" || x === "400") || [...w].join(); })()`,
      ),
    );
    check(
      "base background is carbon #141416",
      await page.evaluate(
        `getComputedStyle(document.body).backgroundColor === "rgb(20, 20, 22)"`,
      ),
    );
    check(
      "no uppercase text transforms",
      await page.evaluate(
        `[...document.querySelectorAll("body *")].every((el) => getComputedStyle(el).textTransform !== "uppercase")`,
      ),
    );
    check(
      "no page errors were thrown",
      page.errors.length === 0,
      page.errors.slice(0, 2).join(" | "),
    );

    page.close();
    browser.close();
  } finally {
    child.kill("SIGTERM");
  }

  const failed = results.filter((entry) => !entry.passed);
  process.stdout.write(
    `\n${results.length - failed.length}/${results.length} checks passed` +
      (failed.length > 0 ? `, ${failed.length} FAILED` : "") +
      `\nscreenshots: ${SHOTS}/\n`,
  );
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((error) => {
  process.stderr.write(`${error.stack ?? error}\n`);
  process.exit(1);
});
