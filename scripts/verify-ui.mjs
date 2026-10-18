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
  F10: ["F10", 121],
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
  // A control inside a scrolling panel may be below the fold; a pointer at its
  // coordinates would hit whatever is there instead.
  await cdp.evaluate(
    `document.querySelector(${JSON.stringify(selector)})?.scrollIntoView({ block: "nearest" })`,
  );
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

/** The PDF page box in the preview header: `{ page, of }` (1-based), or null. */
const PAGE_BOX = `(() => { const i = document.querySelector('[data-page-input]'); if (!i) return null; const m = /\\/\\s*(\\d+)/.exec(i.closest("label")?.innerText ?? ""); return { page: Number(i.value), of: Number(m?.[1] ?? 0) }; })()`;

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
      "home nav lists Projects, Recent, Pinned, Archive, Templates, Trash and Settings",
      await page.evaluate(
        `[...document.querySelectorAll('nav[aria-label="Home sections"] button')].map((b) => b.getAttribute("aria-label")).join() === "Projects,Recent,Pinned,Archive,Templates,Trash,Settings"`,
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
    await sleep(150);
    check(
      "Templates section renders starter templates",
      (await page.evaluate(
        `document.querySelectorAll('[data-template-card]').length`,
      )) >= 11,
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
        `[...document.querySelectorAll('[role="menu"] [role="menuitem"]')].map((i) => i.textContent.trim()).join() === "Open,Rename,Edit tags,Pin,Archive,Copy path,Move to trash"`,
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
    section("Home trash");

    // Move a project to trash via card context menu.
    await page.evaluate(
      `document.querySelector('[data-project$="lecture-notes"] button').dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 300 }))`,
    );
    await sleep(200);
    await clickText(page, '[role="menuitem"]', "Move to trash");
    await sleep(200);
    check(
      "trashed project leaves Projects list",
      !(await cards(page)).includes("lecture-notes"),
    );
    check(
      "Trash nav shows count 1",
      await page.evaluate(
        `document.querySelector('nav[aria-label="Home sections"] button[aria-label="Trash"]')?.textContent.includes("1") ?? false`,
      ),
    );

    // Navigate to Trash section.
    await clickText(page, 'nav[aria-label="Home sections"] button', "Trash");
    await sleep(200);
    check(
      "Trash section lists the trashed project",
      (await cards(page)).includes("lecture-notes"),
    );
    check(
      "trashed card shows relative trash time and Restore/Delete buttons",
      await page.evaluate(`(() => {
        const card = document.querySelector('[data-project$="lecture-notes"]');
        if (!card) return false;
        const text = card.innerText;
        const hasTime = text.includes("Trashed");
        const hasRestore = !!card.querySelector('button[aria-label^="Restore"]');
        const hasDelete = !!card.querySelector('button[aria-label^="Delete"]');
        return hasTime && hasRestore && hasDelete;
      })()`),
    );
    await shoot(page, "05b-home-trash");

    // Restore brings it back to Projects.
    await clickText(page, '[data-project$="lecture-notes"] button', "Restore");
    await sleep(200);
    check("after restore, Trash section is empty", (await cards(page)).length === 0);
    await clickText(page, 'nav[aria-label="Home sections"] button', "Projects");
    await sleep(200);
    check(
      "restored project is back in Projects",
      (await cards(page)).includes("lecture-notes"),
    );

    // Re-trash and purge ("Remove from Manang").
    await page.evaluate(
      `document.querySelector('[data-project$="lecture-notes"] button').dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 300 }))`,
    );
    await sleep(200);
    await clickText(page, '[role="menuitem"]', "Move to trash");
    await sleep(200);
    await clickText(page, 'nav[aria-label="Home sections"] button', "Trash");
    await sleep(200);

    // Click Delete… on the card.
    await clickText(page, '[data-project$="lecture-notes"] button', "Delete…");
    await sleep(200);
    check(
      "delete dialog asks how to remove",
      await page.evaluate(
        `document.querySelector('[role="alertdialog"]')?.textContent.includes("Remove from Manang") ?? false`,
      ),
    );
    // Choose "Remove from Manang".
    await clickText(page, '[role="alertdialog"] button', "Remove from Manang");
    await sleep(200);
    check("purged project is gone from Trash", (await cards(page)).length === 0);

    // Folder still exists in mock, so typing the path can find it.
    await clickText(page, "button", "Open folder");
    await sleep(250);
    await click(page, 'input[placeholder*="my-paper"]');
    await insertText(page, "/home/author/lecture-notes");
    await press(page, "Enter", { wait: 500 });
    check(
      "folder still exists and can be re-opened",
      await page.evaluate(`!document.querySelector('[role="alert"]')`),
    );
    await page.evaluate(`localStorage.removeItem("manang.session.v1")`);

    /* ------------------------------------------------------------------ */
    section("Templates");
    // The sections before this one may have left a project open: start from Home.
    if (!(await exists(page, 'section[aria-label="Home"]'))) {
      await click(page, 'header button[aria-label="Home"]');
      await waitFor(page, `!!document.querySelector('section[aria-label="Home"]')`, {
        label: "Home shown",
      });
    }
    await clickText(page, 'nav[aria-label="Home sections"] button', "Templates");
    await sleep(200);

    const templateCards = await page.evaluate(
      `document.querySelectorAll('[data-template-card]').length`,
    );
    check(
      "templates section renders all starter templates",
      templateCards >= 11,
      `count: ${templateCards}`,
    );

    // Filter by LaTeX
    await clickText(
      page,
      '[aria-label="Filter templates by language"] button',
      "LaTeX",
    );
    await sleep(150);
    const latexCount = await page.evaluate(
      `document.querySelectorAll('[data-template-card]').length`,
    );
    const onlyLatex = await page.evaluate(
      `[...document.querySelectorAll('[data-template-card]')].every((c) => c.innerText.includes("LaTeX"))`,
    );
    check(
      "filtering by LaTeX shows only LaTeX templates",
      onlyLatex && latexCount === 3,
      `count: ${latexCount}`,
    );

    // Filter by All
    await clickText(page, '[aria-label="Filter templates by language"] button', "All");
    await sleep(150);
    const allCount = await page.evaluate(
      `document.querySelectorAll('[data-template-card]').length`,
    );
    check("resetting filter shows all templates", allCount >= 11, `count: ${allCount}`);

    // Click "Use template" on Academic Article
    const articleCardExists = await page.evaluate(
      `!!document.querySelector('[data-template-card="article-typst"]')`,
    );
    check("article-typst card is present", articleCardExists);

    await clickText(
      page,
      '[data-template-card="article-typst"] button',
      "Use template",
    );
    await sleep(250);

    check(
      "clicking Use template opens New project dialog with template preselected",
      await page.evaluate(`(() => {
        const dialog = document.querySelector('[role="dialog"]');
        if (!dialog) return false;
        const select = dialog.querySelector('select[aria-label="Template"]');
        return select && select.value === "article-typst";
      })()`),
    );

    // Create a new project from this template
    await click(page, 'input[placeholder="Entropy paper"]');
    await insertText(page, "verify-template-paper");
    await sleep(150);
    await clickText(page, 'button[type="submit"]', "Create project");
    await sleep(500);

    await waitFor(
      page,
      `document.querySelector(".cm-content")?.textContent.includes("Introduction")`,
      { label: "editor shows template content" },
    );
    check(
      "new project from template opened in workbench with main.typ",
      (await activeTab(page)) === "main.typ",
    );
    await shoot(page, "02b-template-created");

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
      "preview shows the Typst document as a PDF from typst.export-pdf",
      `document.querySelector('[data-pdf-viewer] canvas') !== null`,
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
      "the title bar has the logo only: no name, no breadcrumb",
      await page.evaluate(
        `(() => { const b = document.querySelector('header button[aria-label="Home"]'); return b.innerText.trim() === "" && !document.querySelector('header nav[aria-label="Breadcrumb"]') && !!b.querySelector("svg"); })()`,
      ),
    );
    {
      const logo = await rect(page, 'header button[aria-label="Home"]');
      const colour = () =>
        page.evaluate(
          `getComputedStyle(document.querySelector('header button[aria-label="Home"] svg')).color`,
        );
      const bg = () =>
        page.evaluate(
          `getComputedStyle(document.querySelector('header button[aria-label="Home"]')).backgroundColor`,
        );
      const before = await colour();
      await mouse(page, "mouseMoved", logo.x, logo.y);
      await sleep(250);
      check(
        "hovering the logo turns it lime, without a background",
        (await colour()) !== before &&
          (await colour()) === "rgb(198, 241, 53)" &&
          ["rgba(0, 0, 0, 0)", "transparent"].includes(await bg()),
        `${before} -> ${await colour()}, bg ${await bg()}`,
      );
      await mouse(page, "mouseMoved", 640, 400);
      // Every icon button turns lime on hover, with no background: a shared
      // icon button (the Files panel header) and an activity-bar button. (The
      // window controls only exist in the desktop app.)
      for (const [name, selector] of [
        ["a panel icon button", "#side-panel button.mng-icon-btn"],
        [
          "an activity-bar button",
          'nav[aria-label="Workbench views"] button[aria-label="Search"]',
        ],
      ]) {
        const target = await rect(page, selector);
        if (target === null) {
          check(`hover colour: ${name} exists`, false);
          continue;
        }
        await mouse(page, "mouseMoved", target.x, target.y);
        await sleep(250);
        const seen = await page.evaluate(
          `(() => { const s = getComputedStyle(document.querySelector(${JSON.stringify(selector)})); return s.color + " | " + s.backgroundColor; })()`,
        );
        check(
          `hovering ${name} turns it lime, without a background`,
          seen.startsWith("rgb(198, 241, 53)") &&
            /\| (rgba\(0, 0, 0, 0\)|transparent)$/.test(seen),
          seen,
        );
        await mouse(page, "mouseMoved", 640, 400);
      }
    }
    check(
      "the window title names the file and the project",
      await page.evaluate(`document.title === "main.typ — Entropy paper"`),
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
        `getComputedStyle(document.querySelector('[data-pdf-viewer] [data-page="1"]')).backgroundColor === "rgb(250, 249, 247)"`,
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
    // Opened over a document, Settings show that document's settings; this
    // part checks the whole page, so widen to all settings first.
    if (await exists(page, "[data-settings-scope]"))
      await clickText(page, "[data-settings-scope] button", "Show all settings");
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
    section("Settings views");
    await click(
      page,
      'nav[aria-label="Workbench views"] button[aria-label="Settings"]',
    );
    await waitFor(page, `!!document.querySelector('section[aria-label="Settings"]')`);
    await clickText(
      page,
      'section[aria-label="Settings"] button',
      "Keyboard shortcuts",
    );
    await expectEventually(
      page,
      "Keyboard shortcuts lists a command with two shortcuts as two rows",
      `(() => { const t = document.querySelector('section[aria-label="Settings"]').innerText; return t.includes("Ctrl+K") && t.includes("Ctrl+Shift+P"); })()`,
      {
        show: `document.querySelector('section[aria-label="Settings"]')?.innerText.slice(0, 300)`,
      },
    );
    check(
      "no source-comment text leaks onto the page while Settings are open",
      await page.evaluate(`!/\\/\\*|\\*\\/|<!--/.test(document.body.innerText)`),
    );
    await clickText(page, 'section[aria-label="Settings"] button', "About");
    await expectEventually(
      page,
      "About shows the app and its licences",
      `(() => { const t = document.querySelector('section[aria-label="Settings"]').innerText; return t.includes("Manang") && /licen[cs]e/i.test(t); })()`,
      {
        show: `document.querySelector('section[aria-label="Settings"]')?.innerText.slice(0, 300)`,
      },
    );
    await press(page, "Escape");
    await press(page, "Escape");
    await expectEventually(
      page,
      "Esc leaves Settings from the About view",
      `!document.querySelector('section[aria-label="Settings"]')`,
    );

    /* ------------------------------------------------------------------ */
    section("Contextual settings");
    const settingsGroups = () =>
      page.evaluate(
        `[...document.querySelectorAll('section[aria-label="Settings"] [data-group]')].map((e) => e.dataset.group)`,
      );
    const settingsNav = () =>
      page.evaluate(
        `document.querySelector('nav[aria-label="Settings sections"]')?.innerText ?? ""`,
      );
    const scopeLine = () =>
      page.evaluate(`document.querySelector("[data-settings-scope]")?.innerText ?? ""`);
    const closeSettings = async () => {
      for (
        let i = 0;
        i < 3 && (await exists(page, 'section[aria-label="Settings"]'));
        i++
      )
        await press(page, "Escape");
      await waitFor(page, `!document.querySelector('section[aria-label="Settings"]')`, {
        label: "settings closed",
      });
    };
    const openSettingsFromBar = async () => {
      await click(
        page,
        'nav[aria-label="Workbench views"] button[aria-label="Settings"]',
      );
      await waitFor(
        page,
        `!!document.querySelector('section[aria-label="Settings"] [data-group]')`,
        {
          label: "settings open",
        },
      );
    };

    await press(page, "p", { ctrl: true, wait: 300 });
    await insertText(page, "main.typ");
    await press(page, "Enter", { wait: 600 });
    await waitFor(
      page,
      `document.querySelector('[role="tab"][data-tab][aria-selected="true"]')?.dataset.tab === "main.typ"`,
      {
        label: "main.typ active",
      },
    );
    await openSettingsFromBar();
    await expectEventually(
      page,
      "from a Typst document, the scope line names the file and its engine",
      `document.querySelector("[data-settings-scope]")?.innerText.includes("Settings for main.typ · Typst")`,
      { show: `document.querySelector("[data-settings-scope]")?.innerText` },
    );
    await expectEventually(
      page,
      "a Typst document shows Editor, Preview, Compile, Typst, Writing and Citations",
      `(() => { const g = [...document.querySelectorAll('section[aria-label="Settings"] [data-group]')].map((e) => e.dataset.group); return ["editor", "preview", "compile", "plugin:typst", "plugin:writing", "plugin:citations"].every((x) => g.includes(x)); })()`,
      {
        show: `[...document.querySelectorAll('section[aria-label="Settings"] [data-group]')].map((e) => e.dataset.group).join()`,
      },
    );
    {
      const groups = await settingsGroups();
      check(
        "and not Appearance, LaTeX or Search",
        !["appearance", "files", "plugin:latex", "plugin:search", "plugin:figure"].some(
          (x) => groups.includes(x),
        ),
        groups.join(),
      );
      const nav = await settingsNav();
      check(
        "the section list holds only the shown groups",
        ["Editor", "Preview", "Compile", "Typst", "Writing", "Citations"].every((n) =>
          nav.includes(n),
        ) &&
          !["Appearance", "LaTeX", "Search", "Keyboard shortcuts", "About"].some((n) =>
            nav.includes(n),
          ),
        nav.replace(/\s+/g, " "),
      );
      check(
        "Keyboard shortcuts and About stay reachable as links",
        await page.evaluate(
          `(() => { const t = document.querySelector("[data-settings-app-links]")?.innerText ?? ""; return t.includes("Keyboard shortcuts") && t.includes("About"); })()`,
        ),
      );
    }
    await shoot(page, "07c-settings-document");

    // Search finds in the scope first, and links what only all settings show.
    await click(page, 'header input[aria-label="Search settings and plugins"]');
    await insertText(page, "theme");
    await expectEventually(
      page,
      "searching for a setting outside the scope offers it in all settings",
      `/\\d+ more in all settings/.test(document.querySelector("[data-settings-more]")?.innerText ?? "") && !document.querySelector('[data-setting="appearance.theme"]')`,
      {
        show: `document.querySelector('section[aria-label="Settings"]').innerText.slice(0, 300)`,
      },
    );
    await shoot(page, "07d-settings-search-elsewhere");
    await clickText(page, "[data-settings-more] button", "more in all settings");
    await expectEventually(
      page,
      "following the link shows the match in all settings, without closing",
      `!!document.querySelector('[data-setting="appearance.theme"]') && !document.querySelector("[data-settings-scope]")`,
    );
    await closeSettings();
    check(
      "Escape returns to the same tab",
      (await activeTab(page)) === "main.typ",
      await activeTab(page),
    );

    // Show all settings widens the page without closing it.
    await press(page, ",", { ctrl: true, wait: 400 });
    await expectEventually(
      page,
      "Ctrl+, over a document opens that document's settings",
      `document.querySelector("[data-settings-scope]")?.innerText.includes("main.typ")`,
    );
    await clickText(page, "[data-settings-scope] button", "Show all settings");
    await expectEventually(
      page,
      "Show all settings shows every group and keeps the page open",
      `(() => { const g = [...document.querySelectorAll('section[aria-label="Settings"] [data-group]')].map((e) => e.dataset.group); return ["appearance", "editor", "files", "plugin:latex", "plugin:search", "plugin:typst"].every((x) => g.includes(x)) && !document.querySelector("[data-settings-scope]"); })()`,
    );
    check(
      "all settings list Keyboard shortcuts and About in the section list",
      await page.evaluate(
        `(() => { const t = document.querySelector('nav[aria-label="Settings sections"]').innerText; return t.includes("Keyboard shortcuts") && t.includes("About"); })()`,
      ),
    );
    await closeSettings();

    // The palette's Document settings and Open all settings.
    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "document settings");
    await press(page, "Enter", { wait: 500 });
    await expectEventually(
      page,
      "Document settings opens the active document's settings",
      `document.querySelector("[data-settings-scope]")?.innerText.includes("main.typ")`,
    );
    await closeSettings();
    await press(page, "k", { ctrl: true, wait: 300 });
    await insertText(page, "open all settings");
    await press(page, "Enter", { wait: 500 });
    await expectEventually(
      page,
      "Open all settings shows everything over a document",
      `!!document.querySelector('section[aria-label="Settings"] [data-group="appearance"]') && !document.querySelector("[data-settings-scope]")`,
    );
    await closeSettings();

    // From a figure: the Figure plugin's settings.
    await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Figures"]');
    await sleep(400);
    await clickText(page, "#side-panel ul button", "state_lattice");
    await waitFor(page, `document.querySelector('svg[role="application"]') !== null`, {
      label: "figure editor",
    });
    await openSettingsFromBar();
    await expectEventually(
      page,
      "from a figure, Settings show the Figure plugin only",
      `(() => { const g = [...document.querySelectorAll('section[aria-label="Settings"] [data-group]')].map((e) => e.dataset.group); return g.includes("plugin:figure") && !g.includes("editor") && !g.includes("appearance") && !g.includes("plugin:typst"); })()`,
      {
        show: `[...document.querySelectorAll('section[aria-label="Settings"] [data-group]')].map((e) => e.dataset.group).join()`,
      },
    );
    check(
      "the scope line names the figure",
      (await scopeLine()).includes("state_lattice.figure · Figure"),
      await scopeLine(),
    );
    await shoot(page, "07e-settings-figure");
    await closeSettings();
    check(
      "closing returns to the figure",
      (await activeTab(page)) === "figures/state_lattice.figure",
    );
    await click(
      page,
      '[role="tab"][data-tab="figures/state_lattice.figure"]',
      "middle",
    );

    // From Home: everything.
    await click(page, 'header button[aria-label="Home"]');
    await waitFor(page, `!!document.querySelector('section[aria-label="Home"]')`, {
      label: "Home shown",
    });
    await click(page, 'nav[aria-label="Home sections"] button[aria-label="Settings"]');
    await expectEventually(
      page,
      "from Home, Settings show every group with no scope line",
      `(() => { const g = [...document.querySelectorAll('section[aria-label="Settings"] [data-group]')].map((e) => e.dataset.group); return ["appearance", "editor", "preview", "compile", "files", "plugin:latex", "plugin:search", "plugin:typst", "plugin:figure"].every((x) => g.includes(x)) && !document.querySelector("[data-settings-scope]"); })()`,
    );
    await closeSettings();
    await clickText(page, "button", "Back to Entropy paper");
    await waitFor(page, `!document.querySelector('section[aria-label="Home"]')`, {
      label: "back in the project",
    });
    await click(
      page,
      'nav[aria-label="Workbench views"] button[aria-label="References"]',
    );

    /* ------------------------------------------------------------------ */
    section("Plugins");
    {
      const bar = 'nav[aria-label="Workbench views"]';
      const rows = () =>
        page.evaluate(
          `[...document.querySelectorAll('#side-panel [data-plugin]')].map((r) => r.dataset.plugin)`,
        );
      const row = (name) =>
        page.evaluate(`(() => {
          const r = document.querySelector('#side-panel [data-plugin="${name}"]');
          if (!r) return null;
          return { status: r.dataset.status, tier: r.dataset.tier, text: r.innerText,
            hasSwitch: !!r.querySelector('button[role="switch"]'),
            checked: r.querySelector('button[role="switch"]')?.getAttribute("aria-checked") ?? null,
            error: r.querySelector("[data-plugin-error]")?.innerText ?? null };
        })()`);
      const rowsWhere = (selector) =>
        page.evaluate(
          `[...document.querySelectorAll('#side-panel [data-plugin]')].filter((r) => ${selector}).map((r) => r.dataset.plugin)`,
        );
      const chip = (id) => click(page, `#side-panel button[data-filter="${id}"]`);
      const filterField = 'input[aria-label="Filter plugins"]';
      const setFilter = async (text) => {
        await page.evaluate(
          `(() => { const f = document.querySelector('${filterField}'); f.value = ${JSON.stringify(text)}; f.dispatchEvent(new Event("input", { bubbles: true })); })()`,
        );
        await sleep(150);
      };
      /** Scroll the element into the panel's view, then click it for real. */
      const tap = async (cdp, selector) => {
        await cdp.evaluate(
          `document.querySelector(${JSON.stringify(selector)})?.scrollIntoView({ block: "center" })`,
        );
        await sleep(120);
        await click(cdp, selector);
      };
      const toast = () =>
        page.evaluate(
          `document.querySelector('footer[aria-label="Status"]')?.innerText ?? ""`,
        );

      await click(page, `${bar} button[aria-label="Plugins"]`);
      await expectEventually(
        page,
        "the Plugins view lists built-in and sample plugins",
        `document.querySelectorAll('#side-panel [data-plugin]').length >= 15`,
        { show: `document.querySelector("#side-panel")?.innerText.slice(0, 200)` },
      );
      check(
        "its panel is named Plugins and the activity icon is pressed",
        (await exists(page, 'aside[aria-label="Plugins panel"]')) &&
          (await page.evaluate(
            `document.querySelector('${bar} button[aria-label="Plugins"]').getAttribute("aria-pressed") === "true"`,
          )),
      );
      await shoot(page, "07f-plugins");

      const editor = await row("editor");
      const git = await row("git");
      const counter = await row("word-count");
      const lint = await row("latex-lint");
      const fetcher = await row("citation-fetch");
      const themes = await row("slide-themes");
      check(
        "built-in plugins say Built in and are enabled",
        editor?.tier === "native" &&
          /Built in/.test(editor.text) &&
          /Enabled/.test(editor.text) &&
          git?.tier === "native" &&
          /Built in/.test(git.text),
      );
      check(
        "installed plugins say Sandboxed",
        /Sandboxed/.test(counter?.text ?? "") && /Sandboxed/.test(lint?.text ?? ""),
      );
      check(
        "states read Enabled, Failed, Needs approval and Disabled",
        counter?.status === "loaded" &&
          /Enabled/.test(counter.text) &&
          lint?.status === "failed" &&
          /Failed/.test(lint.text) &&
          fetcher?.status === "needs_approval" &&
          /Needs approval/.test(fetcher.text) &&
          themes?.status === "disabled" &&
          /Disabled/.test(themes.text),
      );
      check(
        "a failed plugin shows its error, without opening it",
        /imports/.test(lint?.error ?? ""),
        lint?.error ?? "",
      );
      check(
        "the core plugin has no switch; a built-in that may be switched off has one",
        editor?.hasSwitch === false &&
          /Core/.test(editor.text) &&
          git?.hasSwitch === true,
      );
      check(
        "Install is the one lime action of the view",
        await page.evaluate(`(() => {
          const lime = (el) => getComputedStyle(el).backgroundColor === "rgb(198, 241, 53)";
          const all = [...document.querySelectorAll("#side-panel *")].filter(lime);
          return all.length === 1 && all[0].hasAttribute("data-install") && all[0].innerText.trim() === "Install";
        })()`),
      );

      // Search and filter chips.
      await setFilter("latex-li");
      check(
        "searching narrows the list (name, description, command)",
        (await rows()).join() === "latex-lint",
        (await rows()).join(),
      );
      await setFilter("count words");
      check("a command title matches too", (await rows()).join() === "word-count");
      await setFilter("zzzz");
      check(
        "no match says so",
        (await rows()).length === 0 &&
          (await page.evaluate(
            `document.querySelector("#side-panel").innerText.includes("No plugin matches")`,
          )),
      );
      await setFilter("");
      await chip("attention");
      check(
        "Needs attention lists the failed and waiting ones only",
        (await rows()).join() === "citation-fetch,latex-lint,plot-export",
        (await rows()).join(),
      );
      await chip("builtin");
      check(
        "Built in lists first-party plugins only",
        (await rows()).length > 5 &&
          (await rowsWhere(`r.dataset.tier !== "native"`)).length === 0,
      );
      await chip("installed");
      check(
        "Installed lists sandboxed plugins only",
        (await rows()).length === 5 &&
          (await rowsWhere(`r.dataset.tier !== "wasm"`)).length === 0,
        (await rows()).join(),
      );
      await chip("all");

      // Enable and disable.
      await tap(page, '#side-panel [data-plugin="word-count"] button[role="switch"]');
      await expectEventually(
        page,
        "turning a plugin off updates its state",
        `document.querySelector('#side-panel [data-plugin="word-count"]')?.dataset.status === "disabled"`,
      );
      check(
        "and the switch follows",
        (await row("word-count"))?.checked === "false",
        JSON.stringify(await row("word-count")),
      );
      check("with a notice", /Disabled word-count/.test(await toast()), await toast());
      await tap(page, '#side-panel [data-plugin="word-count"] button[role="switch"]');
      await expectEventually(
        page,
        "turning it on again loads it",
        `document.querySelector('#side-panel [data-plugin="word-count"]')?.dataset.status === "loaded"`,
      );

      // Details.
      await tap(page, '#side-panel [data-plugin="word-count"] button[aria-expanded]');
      await expectEventually(
        page,
        "opening a plugin shows what it can access and its commands",
        `(() => { const d = document.querySelector('[data-details="word-count"]'); return !!d && /Reads the open document/.test(d.innerText) && /Count words/.test(d.innerText) && /word-count\\.count/.test(d.innerText); })()`,
      );
      check(
        "an installed plugin can be reloaded and uninstalled",
        (await exists(page, '[data-details="word-count"] [data-action="reload"]')) &&
          (await exists(page, '[data-details="word-count"] [data-action="uninstall"]')),
      );
      await tap(page, '#side-panel [data-plugin="editor"] button[aria-expanded]');
      await expectEventually(
        page,
        "a core plugin's details say it is required",
        `/Required by Manang/.test(document.querySelector('[data-details="editor"]')?.innerText ?? "")`,
      );
      check(
        "and offer no uninstall, disable or reload",
        !(await exists(page, '[data-details="editor"] [data-action="uninstall"]')) &&
          !(await exists(page, '[data-details="editor"] [data-action="toggle"]')) &&
          !(await exists(page, '[data-details="editor"] [data-action="reload"]')),
      );
      await tap(page, '#side-panel [data-plugin="git"] button[aria-expanded]');
      await expectEventually(
        page,
        "a built-in plugin can be switched off but not uninstalled",
        `!!document.querySelector('[data-details="git"] [data-action="toggle"]') && !document.querySelector('[data-details="git"] [data-action="uninstall"]')`,
      );
      await shoot(page, "07g-plugin-details");

      // An update that asks for more shows the new access.
      await tap(page, '#side-panel [data-plugin="plot-export"] button[data-review]');
      await expectEventually(
        page,
        "reviewing an update lists its access with the new line marked New",
        `(() => { const d = document.querySelector('[data-consent="plot-export"]'); return !!d && d.querySelector("[data-consent-new]") !== null && /plots\\.example\\.org/.test(d.innerText); })()`,
      );
      await shoot(page, "07h-plugin-update-consent");
      await press(page, "Escape");
      check(
        "Escape cancels without approving",
        !(await exists(page, '[role="alertdialog"]')) &&
          (await row("plot-export"))?.status === "needs_approval",
      );

      // Uninstall, with confirmation.
      await tap(page, '#side-panel [data-plugin="slide-themes"] button[aria-expanded]');
      await tap(page, '[data-details="slide-themes"] [data-action="uninstall"]');
      await expectEventually(
        page,
        "uninstalling asks first",
        `/Uninstall slide-themes\\?/.test(document.querySelector('[role="alertdialog"]')?.innerText ?? "")`,
      );
      await press(page, "Escape");
      check("cancelling keeps the plugin", (await rows()).includes("slide-themes"));
      await tap(page, '[data-details="slide-themes"] [data-action="uninstall"]');
      await clickText(page, '[role="alertdialog"] button', "Uninstall");
      await expectEventually(
        page,
        "confirming removes it from the list",
        `!document.querySelector('#side-panel [data-plugin="slide-themes"]')`,
      );

      // The install flow: pick, read out the access, approve, load.
      check("word-stats is not installed yet", !(await rows()).includes("word-stats"));
      await tap(page, "#side-panel [data-install]");
      await expectEventually(
        page,
        "Install shows what the plugin asks for before anything is installed",
        `(() => { const d = document.querySelector('[data-consent="word-stats"]'); return !!d && /Reads the open document/.test(d.innerText) && /Install word-stats/.test(d.innerText); })()`,
      );
      await shoot(page, "07i-plugin-install-consent");
      check(
        "nothing was installed while the dialog is open",
        !(await rows()).includes("word-stats"),
      );
      await press(page, "Escape");
      check(
        "cancelling installs nothing",
        !(await rows()).includes("word-stats") &&
          !(await exists(page, '[role="alertdialog"]')),
      );
      await tap(page, "#side-panel [data-install]");
      await waitFor(page, `!!document.querySelector('[data-consent-approve]')`, {
        label: "consent dialog",
      });
      await tap(page, "[data-consent-approve]");
      await expectEventually(
        page,
        "approving installs the plugin and loads it",
        `document.querySelector('#side-panel [data-plugin="word-stats"]')?.dataset.status === "loaded"`,
      );
      check(
        "with a notice and its details open",
        /Installed word-stats/.test(await toast()) &&
          (await exists(page, '[data-details="word-stats"]')),
        await toast(),
      );

      // Live: a change from elsewhere shows without reloading the page.
      await page.evaluate(
        `import("/src/lib/kernel/index.ts").then((m) => m.kernel.run("plugins.disable", { name: "search" }))`,
      );
      await expectEventually(
        page,
        "a plugin switched off elsewhere updates the list by itself",
        `document.querySelector('#side-panel [data-plugin="search"]')?.dataset.status === "disabled"`,
      );
      await page.evaluate(
        `import("/src/lib/kernel/index.ts").then((m) => m.kernel.run("plugins.enable", { name: "search" }))`,
      );
      await expectEventually(
        page,
        "and back",
        `document.querySelector('#side-panel [data-plugin="search"]')?.dataset.status === "loaded"`,
      );

      // The shortcut and the command open this view.
      await click(page, `${bar} button[aria-label="Plugins"]`);
      check("the icon collapses the panel", !(await exists(page, "#side-panel")));
      await press(page, "x", { ctrl: true, shift: true, wait: 400 });
      check(
        "Ctrl+Shift+X opens the Plugins view",
        await exists(page, 'aside[aria-label="Plugins panel"]'),
      );
      check(
        "and no Settings page is opened for it",
        !(await exists(page, 'section[aria-label="Settings"]')),
      );
      check(
        "the page does not scroll sideways",
        await page.evaluate(
          `document.documentElement.scrollWidth <= window.innerWidth`,
        ),
      );
    }

    /* ------------------------------------------------------------------ */
    section("AI agent");
    {
      const SECRET_KEY = "sk-verify-ui-secret-key-0123456789abcdef";
      const pageText = () =>
        page.evaluate(
          `document.body.innerText + [...document.querySelectorAll("input, textarea")].map((i) => i.value).join(" ")`,
        );
      const runCommand = async (title) => {
        await press(page, "k", { ctrl: true, wait: 300 });
        await insertText(page, title);
        await press(page, "Enter", { wait: 400 });
      };
      const closeSettingsPage = async () => {
        for (
          let i = 0;
          i < 3 && (await exists(page, 'section[aria-label="Settings"]'));
          i++
        )
          await press(page, "Escape");
        await waitFor(
          page,
          `!document.querySelector('section[aria-label="Settings"]')`,
          {
            label: "settings closed",
          },
        );
      };
      const openSettingsPage = async () => {
        await click(
          page,
          'nav[aria-label="Workbench views"] button[aria-label="Settings"]',
        );
        await waitFor(
          page,
          `!!document.querySelector('section[aria-label="Settings"] [data-group]')`,
          { label: "settings open" },
        );
      };
      /** Triple-click the editor line containing `text`: selects the whole line. */
      const selectLine = async (text) => {
        const at = await page.evaluate(`(() => {
          const line = [...document.querySelectorAll(".cm-line")].find((l) => l.textContent.includes(${JSON.stringify(text)}));
          if (!line) return null;
          line.scrollIntoView({ block: "center" });
          const r = line.getBoundingClientRect();
          return { x: r.left + 40, y: r.top + r.height / 2 };
        })()`);
        if (at === null) throw new Error(`no editor line containing ${text}`);
        for (const count of [1, 2, 3]) {
          await mouse(page, "mouseMoved", at.x, at.y);
          await mouse(page, "mousePressed", at.x, at.y, "left", count);
          await mouse(page, "mouseReleased", at.x, at.y, "left", count);
        }
        await sleep(200);
      };
      // The document's own text: the diff widget (inserted text and buttons) is not part of it.
      const docText = () =>
        page.evaluate(`(() => {
          const c = document.querySelector(".cm-content")?.cloneNode(true);
          if (!c) return "";
          c.querySelectorAll(".mng-ai-inline").forEach((n) => n.remove());
          return c.textContent;
        })()`);
      const agentLog = () =>
        page.evaluate(`document.querySelector("[data-agent-log]")?.innerText ?? ""`);

      // A document tab to work in.
      if (!(await exists(page, '[role="tab"][data-tab="main.typ"]'))) {
        await press(page, "p", { ctrl: true, wait: 300 });
        await insertText(page, "main.typ");
        await press(page, "Enter", { wait: 500 });
      } else {
        await click(page, '[role="tab"][data-tab="main.typ"]');
      }
      await waitFor(
        page,
        `document.querySelector(".cm-content")?.textContent.includes("generalizes")`,
      );
      const original = await docText();

      // Not set up: the view says so and offers Set up; nothing is sent.
      await runCommand("Show AI agent");
      check(
        "the Agent tab is part of the bottom panel",
        await exists(page, "#panel-tab-agent"),
      );
      check(
        "Agent is a tab, not a new activity-bar item",
        !(await page.evaluate(
          `[...document.querySelectorAll('nav[aria-label="Workbench views"] button')].some((b) => /agent|ai/i.test(b.getAttribute("aria-label") ?? ""))`,
        )),
      );
      await expectEventually(
        page,
        "before set-up the header says not set up and offers Set up",
        `/not set up/.test(document.querySelector("[data-agent-status]")?.innerText ?? "") && !!document.querySelector("[data-agent-setup]")`,
      );
      check(
        "the view labels the provider as a mock in the browser",
        await exists(page, "[data-agent-mock]"),
      );
      await shoot(page, "ai-01-not-set-up");

      // Set up: provider and key in Settings.
      await click(page, "[data-agent-setup]");
      await waitFor(page, `!!document.querySelector('[data-group="agent-provider"]')`, {
        label: "provider panel",
      });
      check(
        "Settings has a Provider and key panel",
        await page.evaluate(
          `document.querySelector('[data-group="agent-provider"]').innerText.includes("Provider and key")`,
        ),
      );
      check(
        "the key field is a password field",
        await page.evaluate(
          `document.querySelector("[data-agent-key]").type === "password"`,
        ),
      );
      await page.evaluate(`document.querySelector("[data-agent-key]").focus()`);
      await insertText(page, SECRET_KEY);
      await click(page, "[data-agent-key-save]");
      await expectEventually(
        page,
        "after saving, the field is empty and the panel only says where the key is held",
        `document.querySelector("[data-agent-key]").value === "" && /Not saved: stored for this session only/.test(document.querySelector("[data-agent-key-status]").innerText)`,
      );
      check(
        "the key is never displayed back",
        !(await pageText()).includes(SECRET_KEY),
      );
      await click(page, "[data-agent-test]");
      await expectEventually(
        page,
        "Test connection answers in plain words",
        `/Connected/.test(document.querySelector("[data-agent-test-result]")?.innerText ?? "")`,
      );
      await shoot(page, "ai-02-provider-and-key");
      await closeSettingsPage();

      // A selection action: consent first, then an inline diff.
      await click(page, ".cm-content");
      await selectLine("generalizes Landauer");
      await runCommand("AI: Shorten selection");
      await expectEventually(
        page,
        "the first request asks for consent and says what is sent and where",
        `(() => { const d = document.querySelector('[role="alertdialog"]'); return !!d && /Anthropic/.test(d.innerText) && /sends the text you attach/.test(d.innerText); })()`,
      );
      await shoot(page, "ai-03-consent");
      await clickText(page, '[role="alertdialog"] button', "Agree");
      await expectEventually(
        page,
        "the result is an inline diff: removed text struck through, inserted text after it",
        `!!document.querySelector(".cm-content .mng-ai-removed") && !!document.querySelector(".cm-content .mng-ai-inserted")`,
        { timeout: 15000 },
      );
      check(
        "removed text is dim and struck through",
        await page.evaluate(
          `getComputedStyle(document.querySelector(".mng-ai-removed")).textDecorationLine.includes("line-through")`,
        ),
      );
      check(
        "inserted text uses the Sage token",
        await page.evaluate(
          `getComputedStyle(document.querySelector(".mng-ai-inserted")).color === getComputedStyle(document.documentElement).getPropertyValue("--mng-math").trim().replace(/^#(..)(..)(..)$/, (_, r, g, b) => "rgb(" + [r, g, b].map((x) => parseInt(x, 16)).join(", ") + ")")`,
        ),
      );
      check(
        "the diff widget has square Accept and Reject buttons and no shadow",
        await page.evaluate(
          `(() => { const b = document.querySelector("[data-ai-accept]"); const s = getComputedStyle(b); return !!document.querySelector("[data-ai-reject]") && s.borderRadius === "0px" && s.boxShadow === "none"; })()`,
        ),
      );
      check("the document is unchanged until Accept", (await docText()) === original);
      check(
        "the Agent view shows the proposal as a card with the one lime action, Accept",
        await page.evaluate(`(() => {
          const accept = document.querySelector("[data-agent-accept]");
          const send = document.querySelector("[data-agent-send]");
          const lime = (el) => getComputedStyle(el).backgroundColor === "rgb(198, 241, 53)";
          return !!accept && lime(accept) && !!send && !lime(send);
        })()`),
      );
      check(
        "the context tray shows the selection chip with a token count",
        await page.evaluate(
          `(() => { const c = document.querySelector('[data-chip="selection"]'); return !!c && /\\d/.test(c.innerText) && /tokens/.test(document.querySelector("[data-tray-tokens]").innerText); })()`,
        ),
      );
      check(
        "the usage line labels cost as an estimate",
        await page.evaluate(
          `/estimate/.test(document.querySelector("[data-agent-usage]").innerText)`,
        ),
      );
      await shoot(page, "ai-04-inline-diff");

      // Preview what will be sent.
      await click(page, "[data-agent-preview]");
      await expectEventually(
        page,
        "Preview shows the exact payload with the selected text",
        `(() => { const d = document.querySelector("[data-agent-preview-dialog]"); return !!d && /What will be sent/.test(d.innerText) && /generalizes Landauer/.test(d.querySelector("[data-preview-body]")?.innerText ?? ""); })()`,
      );
      check(
        "the preview is read-only and reports tokens and the redaction count",
        await page.evaluate(
          `(() => { const d = document.querySelector("[data-agent-preview-dialog]"); return !d.querySelector("input, textarea") && !!d.querySelector("[data-preview-tokens]") && !!d.querySelector("[data-redactions]"); })()`,
        ),
      );
      await shoot(page, "ai-05-preview");
      await press(page, "Escape");
      check(
        "Esc closes the preview",
        !(await exists(page, "[data-agent-preview-dialog]")),
      );

      // Accept: one transaction, undone by one Ctrl+Z.
      await click(page, "[data-ai-accept]");
      await expectEventually(
        page,
        "Accept applies the edit and removes the diff",
        `!document.querySelector(".cm-content .mng-ai-removed") && !document.querySelector("[data-ai-accept]")`,
      );
      const accepted = await docText();
      check(
        "the text changed",
        accepted !== original && accepted.length < original.length,
      );
      await press(page, "z", { ctrl: true, wait: 300 });
      check("one Ctrl+Z restores the original text", (await docText()) === original);
      check("and nothing else was left edited", (await dirtyTabs(page)).length === 0);

      // Reject with Esc leaves the text alone.
      await selectLine("generalizes Landauer");
      await runCommand("AI: Rewrite selection");
      await expectEventually(
        page,
        "Rewrite also proposes an inline diff",
        `!!document.querySelector("[data-ai-accept]")`,
        { timeout: 15000 },
      );
      await page.evaluate(`document.querySelector(".cm-content").focus()`);
      await press(page, "Escape", { wait: 400 });
      check(
        "Esc rejects: the diff goes and the text is untouched",
        !(await exists(page, "[data-ai-accept]")) && (await docText()) === original,
      );
      check("the card says it was rejected", (await agentLog()).includes("Rejected."));

      // Stop cancels a stream.
      await page.evaluate(`document.querySelector("[data-agent-input]").focus()`);
      await insertText(page, "explain this slow please");
      await click(page, "[data-agent-send]");
      await expectEventually(
        page,
        "while streaming the Send button becomes Stop",
        `!!document.querySelector("[data-agent-stop]")`,
      );
      await click(page, "[data-agent-stop]");
      await expectEventually(
        page,
        "Stop ends the stream with a quiet notice and Send returns",
        `!!document.querySelector("[data-agent-send]") && /Stopped\\./.test(document.querySelector("[data-agent-log]").innerText)`,
      );

      // Fix with AI on a Problems entry.
      await click(page, ".cm-content");
      await press(page, "End", { ctrl: true });
      await insertText(page, "\nA broken formula $x + y\n");
      await expectEventually(
        page,
        "a problem appears",
        `/1 error/.test(document.querySelector('footer[aria-label="Status"]').innerText)`,
        { timeout: 10000 },
      );
      await click(page, "#panel-tab-problems");
      await expectEventually(
        page,
        "the Problems list offers Fix with AI",
        `!!document.querySelector("#bottom-panel [data-fix-with-ai]")`,
      );
      await click(page, "#bottom-panel [data-fix-with-ai]");
      await expectEventually(
        page,
        "Fix with AI sends the diagnostic and proposes an edit as an inline diff",
        `!!document.querySelector(".cm-content [data-ai-accept]")`,
        { timeout: 15000 },
      );
      check(
        "its context tray holds the problem chip, not the whole file",
        await page.evaluate(
          `!!document.querySelector('[data-chip="diagnostics"]') && !document.querySelector('[data-chip="file"]')`,
        ),
      );
      await page.evaluate(`document.querySelector(".cm-content").focus()`);
      await press(page, "Escape", { wait: 300 });
      // Put the document back exactly as it was.
      await click(page, ".cm-content");
      for (let i = 0; i < 4 && (await docText()) !== original; i++)
        await press(page, "z", { ctrl: true, wait: 250 });
      check(
        "the document is restored after the fix test",
        (await docText()) === original,
      );

      // Disabled state.
      await openSettingsPage();
      await click(page, '[data-setting="agent.enabled"] button[role="switch"]');
      await closeSettingsPage();
      await runCommand("Show AI agent");
      await expectEventually(
        page,
        "when disabled the view shows Set up",
        `/off/.test(document.querySelector("[data-agent-status]")?.innerText ?? "") && !!document.querySelector("[data-agent-setup]")`,
      );
      await openSettingsPage();
      await click(page, '[data-setting="agent.enabled"] button[role="switch"]');
      await closeSettingsPage();
      check(
        "the key never appeared anywhere on the page",
        !(await pageText()).includes(SECRET_KEY),
      );
      check(
        "no document is left edited by the AI tests",
        (await dirtyTabs(page)).length === 0,
      );
      // The Agent view made the panel taller; a double click on the resizer restores its size.
      await page.evaluate(
        `document.querySelector('[aria-label="Resize bottom panel"]')?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }))`,
      );
      await sleep(200);
      await click(page, "#bottom-panel button[aria-label='Close panel']");
    }

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
      "the preview keeps the last good pages and says so",
      await page.evaluate(
        `document.querySelector('[data-pdf-viewer] canvas') !== null && document.querySelector('section[aria-label="Preview"]').innerText.includes("last good page")`,
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
    const box = await page.evaluate(PAGE_BOX);
    check(
      "preview toolbar shows page n of N",
      box !== null && box.page === 1 && box.of >= 1,
      JSON.stringify(box),
    );
    const zoomText = () =>
      page.evaluate(
        `document.querySelector("[data-zoom-menu]")?.textContent.trim() ?? ""`,
      );
    const zoomBefore = await zoomText();
    await click(page, 'button[aria-label="Zoom in"]');
    const zoomAfter = await zoomText();
    check(
      "zoom in steps up to the next preset percentage",
      /^\d+%$/.test(zoomAfter) && parseInt(zoomAfter) > parseInt(zoomBefore),
      `${zoomBefore} -> ${zoomAfter}`,
    );
    await click(page, 'button[aria-label="Zoom out"]');
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
      (await activeTab(page)) === "figures/state_lattice.figure" &&
        (await exists(page, '[role="tablist"]')),
    );
    check(
      "a version 1 scene loads (migrated) from the project file",
      await page.evaluate(`document.body.innerText.includes("4 nodes")`),
    );

    {
      const CANVAS = 'svg[role="application"]';
      const FIG_TOOLS = '[role="toolbar"][aria-label="Figure tools"]';
      const nodeIds = (kind) =>
        page.evaluate(
          `[...document.querySelectorAll('${CANVAS} g[data-node-id]${kind === undefined ? "" : `[data-kind="${kind}"]`}')].map((g) => g.dataset.nodeId)`,
        );
      const nodeBox = (id) =>
        page.evaluate(`(() => {
        const g = document.querySelector('${CANVAS} g[data-node-id="${id}"]');
        if (!g) return null;
        const r = g.getBoundingClientRect();
        return { x: r.x, y: r.y, w: r.width, h: r.height, cx: r.x + r.width / 2, cy: r.y + r.height / 2, bottom: r.bottom, right: r.right };
      })()`);
      const nodeTotal = async () => (await nodeIds()).length;
      // The plugin's own copy of the open figure: the canvas culls what is out
      // of view, so DOM counts change when the view does.
      const kernelEval = (body) =>
        page.evaluate(`(async () => {
          const { kernel } = await import("/src/lib/kernel/index.ts");
          const list = await kernel.run("figure.list");
          const fig = list.find((f) => (f.path ?? "").endsWith("state_lattice.figure"));
          ${body}
        })()`);
      const sceneNodes = () =>
        kernelEval(`return (await kernel.run("figure.get", { id: fig.id })).nodes;`);
      const sceneCountExpr = `(async () => {
        const { kernel } = await import("/src/lib/kernel/index.ts");
        const list = await kernel.run("figure.list");
        const fig = list.find((f) => (f.path ?? "").endsWith("state_lattice.figure"));
        return (await kernel.run("figure.get", { id: fig.id })).nodes.length;
      })()`;
      const sceneCount = () => page.evaluate(sceneCountExpr);
      async function pickTool(name) {
        await click(page, `${FIG_TOOLS} button[aria-label="${name}"]`);
      }
      async function drag(x1, y1, x2, y2, { button = "left", steps = 4 } = {}) {
        await mouse(page, "mouseMoved", x1, y1);
        await mouse(page, "mousePressed", x1, y1, button);
        for (let i = 1; i <= steps; i += 1) {
          await mouse(
            page,
            "mouseMoved",
            x1 + ((x2 - x1) * i) / steps,
            y1 + ((y2 - y1) * i) / steps,
            button,
          );
        }
        await mouse(page, "mouseReleased", x2, y2, button);
        await sleep(250);
      }
      async function tap(x, y, { shift = false } = {}) {
        await mouse(page, "mouseMoved", x, y);
        await cdpMouse("mousePressed", x, y, shift);
        await cdpMouse("mouseReleased", x, y, shift);
        await sleep(200);
      }
      async function cdpMouse(type, x, y, shift) {
        await page.send("Input.dispatchMouseEvent", {
          type,
          x,
          y,
          button: "left",
          buttons: type === "mousePressed" ? 1 : 0,
          clickCount: 1,
          modifiers: shift ? 8 : 0,
        });
      }

      // Start from a known view: everything in sight.
      await click(page, `${FIG_TOOLS} button[aria-label="Zoom to fit"]`);
      const svg = await rect(page, CANVAS);
      const at = (fx, fy) => ({ x: svg.left + svg.w * fx, y: svg.top + svg.h * fy });

      // Every drawing tool draws its shape. Each lands in an empty strip at
      // the bottom of the view, so the scene's own nodes are not in the way.
      const drawn = {};
      for (const [name, kind, fx] of [
        ["Rectangle", "rect", 0.08],
        ["Diamond", "diamond", 0.3],
        ["Ellipse", "ellipse", 0.52],
        ["Frame", "frame", 0.74],
      ]) {
        const before = await nodeIds(kind);
        await pickTool(name);
        const a = at(fx, 0.8);
        await drag(a.x, a.y, a.x + svg.w * 0.14, a.y + svg.h * 0.12);
        const after = await nodeIds(kind);
        const added = after.filter((id) => !before.includes(id));
        drawn[kind] = added[0];
        check(
          `the ${name} tool draws a ${kind}`,
          added.length === 1,
          `${before} -> ${after}`,
        );
      }
      {
        const before = await nodeIds("line");
        await pickTool("Line");
        const a = at(0.08, 0.95);
        await drag(a.x, a.y, a.x + svg.w * 0.12, a.y);
        check(
          "the Line tool draws a line",
          (await nodeIds("line")).length === before.length + 1,
        );
      }
      {
        const before = await nodeIds("freedraw");
        await pickTool("Draw");
        const a = at(0.3, 0.95);
        await mouse(page, "mouseMoved", a.x, a.y);
        await mouse(page, "mousePressed", a.x, a.y);
        for (let i = 1; i <= 12; i += 1) {
          await mouse(page, "mouseMoved", a.x + i * 8, a.y - Math.sin(i / 2) * 10);
        }
        await mouse(page, "mouseReleased", a.x + 96, a.y);
        await sleep(250);
        check(
          "the Draw tool draws a freehand stroke",
          (await nodeIds("freedraw")).length === before.length + 1,
        );
      }
      {
        const before = await nodeIds("text");
        await pickTool("Text");
        const a = at(0.52, 0.95);
        await tap(a.x, a.y);
        await waitFor(page, `document.querySelector("[data-label-editor]") !== null`, {
          label: "text editor",
          timeout: 4000,
        }).catch(() => undefined);
        await insertText(page, "A note");
        await press(page, "Enter", { ctrl: true, wait: 300 });
        const after = await nodeIds("text");
        check(
          "the Text tool places text typed in place",
          after.length === before.length + 1 &&
            (await page.evaluate(
              `document.querySelector('${CANVAS}').textContent.includes("A note")`,
            )),
        );
      }

      // An arrow from the rectangle to the diamond binds both ends.
      const rectBox = await nodeBox(drawn.rect);
      const diamondBox = await nodeBox(drawn.diamond);
      const arrowsBefore = await nodeIds("arrow");
      await pickTool("Arrow");
      await drag(rectBox.cx, rectBox.cy, diamondBox.cx, diamondBox.cy);
      const arrowId = (await nodeIds("arrow")).find((id) => !arrowsBefore.includes(id));
      check("the Arrow tool draws an arrow between two shapes", arrowId !== undefined);
      await shoot(page, "17-figure");
      const arrowPath = () =>
        page.evaluate(
          `document.querySelector('${CANVAS} g[data-node-id="${arrowId}"] path')?.getAttribute("d") ?? ""`,
        );
      const pathBefore = await arrowPath();
      await pickTool("Select");
      const moveFrom = await nodeBox(drawn.diamond);
      await drag(moveFrom.cx, moveFrom.cy, moveFrom.cx + 40, moveFrom.cy - 90, {
        steps: 6,
      });
      const moved = await nodeBox(drawn.diamond);
      check(
        "dragging a shape moves it (one plugin edit on release)",
        moved !== null && Math.abs(moved.cy - moveFrom.cy) > 40,
        JSON.stringify({ moveFrom, moved }),
      );
      check(
        "the bound arrow follows the shape it is attached to",
        pathBefore !== "" && (await arrowPath()) !== pathBefore,
      );

      // Multi-select with Shift, then align their tops from the properties panel.
      await press(page, "Escape", { wait: 250 });
      const r1 = await nodeBox(drawn.rect);
      const e1 = await nodeBox(drawn.ellipse);
      await tap(r1.cx, r1.cy);
      await tap(e1.cx, e1.cy, { shift: true });
      check(
        "the selection's controls appear in the properties column",
        await exists(page, "aside[data-properties] [data-properties-selection]"),
      );
      await click(
        page,
        'aside[aria-label="Shape properties"] button[aria-label="Align top"]',
      );
      await sleep(300);
      const r2 = await nodeBox(drawn.rect);
      const e2 = await nodeBox(drawn.ellipse);
      check(
        "Align top lines up the selected shapes",
        Math.abs(r2.y - e2.y) < 2.5,
        `${r2.y} vs ${e2.y}`,
      );
      await shoot(page, "17b-figure-properties");

      // Group: clicking one member selects the group, so Delete removes both.
      await click(
        page,
        'aside[aria-label="Shape properties"] button[aria-label="Group"]',
      );
      await sleep(300);
      const empty = at(0.97, 0.05);
      await tap(empty.x, empty.y);
      check(
        "the selection's controls give way to the page settings when nothing is selected",
        !(await exists(page, "aside[data-properties] [data-properties-selection]")) &&
          (await exists(page, "aside[data-properties] [data-page-settings]")),
      );
      const total = await nodeTotal();
      await tap(r2.cx, r2.cy);
      await press(page, "Delete", { wait: 300 });
      check(
        "a grouped shape is picked with its group",
        (await nodeTotal()) === total - 2,
      );
      await press(page, "z", { ctrl: true, wait: 400 });
      check("undo brings the group back", (await nodeTotal()) === total);
      await tap(r2.cx, r2.cy);
      await press(page, "g", { ctrl: true, shift: true, wait: 300 });
      await tap(empty.x, empty.y);
      await tap(r2.cx, r2.cy);
      await press(page, "Delete", { wait: 300 });
      check(
        "after ungrouping a shape is picked alone",
        (await nodeTotal()) === total - 1,
      );
      await press(page, "z", { ctrl: true, wait: 400 });

      // Zoom with Ctrl+wheel and pan with the middle button.
      const zoomText = () =>
        page.evaluate(
          `document.querySelector("[data-zoom]")?.textContent?.trim() ?? ""`,
        );
      const zoomBefore = await zoomText();
      const centre = at(0.5, 0.5);
      await page.send("Input.dispatchMouseEvent", {
        type: "mouseWheel",
        x: centre.x,
        y: centre.y,
        deltaX: 0,
        deltaY: -240,
        modifiers: 2,
      });
      await sleep(300);
      check(
        "Ctrl+wheel zooms the canvas",
        (await zoomText()) !== zoomBefore,
        `${zoomBefore} -> ${await zoomText()}`,
      );
      const panBefore = await nodeBox(drawn.rect);
      await drag(centre.x, centre.y, centre.x + 120, centre.y + 60, {
        button: "middle",
      });
      const panAfter = await nodeBox(drawn.rect);
      check(
        "dragging with the middle button pans",
        Math.abs(panAfter.x - panBefore.x - 120) < 3 &&
          Math.abs(panAfter.y - panBefore.y - 60) < 3,
        JSON.stringify({ panBefore, panAfter }),
      );
      await click(page, `${FIG_TOOLS} button[aria-label="Zoom to fit"]`);

      // Pasting Mermaid offers to turn it into shapes.
      const countBeforeMermaid = await sceneCount();
      await page.evaluate(`(() => {
      const target = document.querySelector('${CANVAS}');
      target.focus();
      const data = new DataTransfer();
      data.setData("text/plain", "flowchart TD\\n  A[Collect] --> B{Enough?}\\n  B -->|yes| C[Analyse]\\n");
      target.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
    })()`);
      await expectEventually(
        page,
        "pasting Mermaid text offers to insert it as shapes",
        `document.querySelector("[data-mermaid-prompt]") !== null`,
      );
      await clickText(page, "[data-mermaid-prompt] button", "Insert as shapes");
      await expectEventually(
        page,
        "Insert as shapes adds native shapes and bound arrows",
        `${sceneCountExpr}.then((n) => n === ${countBeforeMermaid + 5})`,
        { show: sceneCountExpr },
      );
      check(
        "the Mermaid labels are on the canvas",
        await page.evaluate(
          `document.querySelector('${CANVAS}').textContent.includes("Enough?")`,
        ),
      );
      await shoot(page, "17c-figure-mermaid");
      await press(page, "z", { ctrl: true, wait: 400 });
      check(
        "undo takes the whole Mermaid insert back",
        (await sceneCount()) === countBeforeMermaid,
      );
      await press(page, "z", { ctrl: true, shift: true, wait: 400 });
      check("redo restores it", (await sceneCount()) === countBeforeMermaid + 5);

      check(
        "drawing marks the tab dirty",
        (await dirtyTabs(page)).includes("figures/state_lattice.figure"),
      );
      await press(page, "s", { ctrl: true, wait: 500 });
      check(
        "Ctrl+S saves the scene as JSON and its SVG",
        !(await dirtyTabs(page)).includes("figures/state_lattice.figure"),
      );

      /* ---------------------------------------------------------------- */
      /* Formulas, colours and the polish items (figure studio follow-up) */
      /* ---------------------------------------------------------------- */
      section("Figure studio: formulas, colours, layout");
      const PROPS = "aside[data-properties]";
      const exportSvg = () =>
        kernelEval(`
          const bytes = await kernel.runBinary("figure.export-svg", { id: fig.id });
          return new TextDecoder().decode(bytes);`);
      async function dbl(x, y) {
        await mouse(page, "mouseMoved", x, y);
        await mouse(page, "mousePressed", x, y, "left", 1);
        await mouse(page, "mouseReleased", x, y, "left", 1);
        await mouse(page, "mousePressed", x, y, "left", 2);
        await mouse(page, "mouseReleased", x, y, "left", 2);
        await sleep(350);
      }
      async function shiftDrag(x1, y1, x2, y2) {
        const ev = (type, x, y) =>
          page.send("Input.dispatchMouseEvent", {
            type,
            x,
            y,
            button: "left",
            buttons: type === "mouseReleased" ? 0 : 1,
            clickCount: 1,
            modifiers: 8,
          });
        await ev("mouseMoved", x1, y1);
        await ev("mousePressed", x1, y1);
        for (let i = 1; i <= 5; i += 1) {
          await ev("mouseMoved", x1 + ((x2 - x1) * i) / 5, y1 + ((y2 - y1) * i) / 5);
        }
        await ev("mouseReleased", x2, y2);
        await sleep(300);
      }
      const attr = (id, sel, name) =>
        page.evaluate(
          `document.querySelector('${CANVAS} g[data-node-id="${id}"] ${sel}')?.getAttribute("${name}") ?? null`,
        );
      const clearSelection = async () => {
        const e = at(0.97, 0.02);
        await tap(e.x, e.y);
      };
      await click(page, `${FIG_TOOLS} button[aria-label="Zoom to fit"]`);
      await clearSelection();

      // --- The properties column docks to the right and never covers the drawing.
      {
        const s0 = await rect(page, CANVAS);
        const p0 = await rect(page, PROPS);
        check(
          "the properties column is docked to the right edge of the canvas area",
          p0 !== null &&
            p0.w >= 200 &&
            p0.w <= 300 &&
            Math.abs(p0.right - 1280) < 4 &&
            s0.right <= p0.left + 1,
          JSON.stringify({ s0, p0 }),
        );
        const ids = await nodeIds();
        let covered = 0;
        for (const id of ids) {
          const b = await nodeBox(id);
          if (b === null || b.w === 0) continue;
          const hit = await page.evaluate(
            `!!document.elementFromPoint(${b.cx}, ${b.cy})?.closest('${PROPS}')`,
          );
          if (hit) covered += 1;
        }
        check("no shape is hidden behind the properties column", covered === 0);
        check(
          "with nothing selected the column holds the page settings",
          await exists(page, `${PROPS} [data-page-settings]`),
        );
        await click(page, `${PROPS} [data-properties-toggle]`);
        const collapsed = await rect(page, PROPS);
        const s1 = await rect(page, CANVAS);
        check(
          "the column collapses to a strip and the canvas takes the room",
          collapsed.w < 48 && s1.w > s0.w + 150,
          JSON.stringify({ before: s0.w, after: s1.w, strip: collapsed.w }),
        );
        await click(page, `${PROPS} [data-properties-toggle]`);
        check("the toggle opens it again", (await rect(page, PROPS)).w >= 200);
        await click(page, `${FIG_TOOLS} button[aria-label="Zoom to fit"]`);
      }

      // --- Typst formula: tool, inline editor, live preview, measured box.
      const mathIds = () => nodeIds("math");
      const mathBefore = await mathIds();
      const mathCountBefore = mathBefore.length;
      await pickTool("Formula");
      const f1At = at(0.1, 0.42);
      await tap(f1At.x, f1At.y);
      await expectEventually(
        page,
        "the formula tool creates a formula and opens its inline editor",
        `document.querySelector("[data-formula-editor]") !== null && document.querySelectorAll('${CANVAS} g[data-kind="math"]').length === ${mathCountBefore + 1}`,
      );
      check(
        "the editor defaults to the project's engine (Typst) and offers both languages",
        await page.evaluate(
          `document.querySelector('[data-formula-editor] button[data-language="typst"]')?.getAttribute("aria-pressed") === "true" && document.querySelector('[data-formula-editor] button[data-language="latex"]')?.getAttribute("aria-pressed") === "false"`,
        ),
      );
      check(
        "the editor field is DM Mono and the help line shows Typst and LaTeX syntax",
        await page.evaluate(
          `(() => { const t = document.querySelector('[data-formula-editor] textarea'); return getComputedStyle(t).fontFamily.includes("DM Mono") && document.querySelector("[data-formula-editor]").innerText.includes("Typst:"); })()`,
        ),
      );
      await insertText(page, "a^2 + b^2 = c^2");
      await expectEventually(
        page,
        "typing re-renders the formula on the canvas after a short pause",
        `[...document.querySelectorAll('${CANVAS} [data-math-image]')].some((i) => i.getAttribute("href")?.startsWith("blob:"))`,
      );
      await shoot(page, "17d-figure-formula-editor");
      await press(page, "Enter", { ctrl: true, wait: 500 });
      const mathNow = await mathIds();
      const f1 = mathNow.find((id) => !mathBefore.includes(id));
      check(
        "Ctrl+Enter applies the formula and closes the editor",
        !(await exists(page, "[data-formula-editor]")) && f1 !== undefined,
      );
      const f1Box = await nodeBox(f1);
      check(
        "a formula is a selectable node with a measured box",
        f1Box !== null && f1Box.w > 20 && f1Box.h > 10,
        JSON.stringify(f1Box),
      );
      check(
        "the formula is drawn as an image of vector paths, not as text",
        (await page.evaluate(
          `document.querySelectorAll('${CANVAS} g[data-node-id="${f1}"] text').length`,
        )) === 0 &&
          (await attr(f1, "[data-math-image]", "data-math-language")) === "typst",
      );

      // --- LaTeX formula, and the language switch re-renders.
      await pickTool("Formula");
      const f2At = at(0.34, 0.42);
      await tap(f2At.x, f2At.y);
      await waitFor(page, `document.querySelector("[data-formula-editor]") !== null`);
      await click(page, '[data-formula-editor] button[data-language="latex"]');
      await insertText(page, "\\frac{1}{2} + \\sqrt{x}");
      await expectEventually(
        page,
        "switching to LaTeX re-renders as a LaTeX formula",
        `!!document.querySelector('${CANVAS} [data-math-image][data-math-language="latex"]')`,
      );
      // The editor's textarea lost focus to the button; a click on the canvas commits.
      const empty2 = at(0.97, 0.02);
      await tap(empty2.x, empty2.y);
      await sleep(300);
      const f2 = (await mathIds()).find((id) => !mathNow.includes(id));
      check(
        "clicking away applies the formula (LaTeX)",
        f2 !== undefined && !(await exists(page, "[data-formula-editor]")),
      );
      // Align them: both are formula nodes the alignment machinery accepts.
      const b1 = await nodeBox(f1);
      const b2 = await nodeBox(f2);
      await tap(b1.cx, b1.cy);
      await tap(b2.cx, b2.cy, { shift: true });
      await click(page, `${PROPS} button[aria-label="Align middle"]`);
      await sleep(300);
      const a1 = await nodeBox(f1);
      const a2 = await nodeBox(f2);
      check(
        "formulas align like any shape",
        Math.abs(a1.cy - a2.cy) < 2.5,
        `${a1.cy} vs ${a2.cy}`,
      );
      await clearSelection();

      // --- An invalid source shows the error and keeps the old rendering, dimmed.
      {
        const c = await nodeBox(f2);
        await dbl(c.cx, c.cy);
        await waitFor(page, `document.querySelector("[data-formula-editor]") !== null`);
        check(
          "double-clicking a formula edits it, on its own language",
          await page.evaluate(
            `document.querySelector('[data-formula-editor] button[data-language="latex"]')?.getAttribute("aria-pressed") === "true"`,
          ),
        );
        await insertText(page, "\\frac{1");
        await expectEventually(
          page,
          "an invalid formula shows its error inside the editor, with a position",
          `/line 1/.test(document.querySelector("[data-formula-error]")?.innerText ?? "")`,
          { show: `document.querySelector("[data-formula-error]")?.innerText` },
        );
        check(
          "the previous rendering stays on screen, dimmed",
          (await page.evaluate(
            `Number(document.querySelector('${CANVAS} g[data-node-id="${f2}"] [data-math-image]')?.getAttribute("opacity") ?? 1)`,
          )) < 0.6,
        );
        await shoot(page, "17e-figure-formula-error");
        await press(page, "Escape", { wait: 300 });
        check(
          "Escape drops the draft and restores the rendering",
          !(await exists(page, "[data-formula-editor]")) &&
            (await page.evaluate(
              `Number(document.querySelector('${CANVAS} g[data-node-id="${f2}"] [data-math-image]')?.getAttribute("opacity") ?? 1)`,
            )) === 1,
        );
        // Committed through the panel, a broken source is stored and reported there.
        await tap(c.cx, c.cy);
        await page.evaluate(`(() => {
          const t = document.querySelector('${PROPS} textarea[aria-label="Formula source"]');
          t.focus();
          t.value = "\\\\frac{1";
          t.dispatchEvent(new Event("change", { bubbles: true }));
        })()`);
        await expectEventually(
          page,
          "a broken formula's error appears in the properties panel",
          `!!document.querySelector('${PROPS} [data-formula-error]')`,
        );
        await page.evaluate(`(() => {
          const t = document.querySelector('${PROPS} textarea[aria-label="Formula source"]');
          t.focus();
          t.value = "\\\\frac{3}{4}";
          t.dispatchEvent(new Event("change", { bubbles: true }));
        })()`);
        await expectEventually(
          page,
          "fixing the source clears the error",
          `!document.querySelector('${PROPS} [data-formula-error]')`,
        );
        await clearSelection();
      }

      // --- Scaling a formula by its handle scales its size.
      {
        const before = await nodeBox(f1);
        await tap(before.cx, before.cy);
        const handle = await rect(page, `${CANVAS} [data-handle="se"]`);
        const sizeBefore = await page.evaluate(
          `document.querySelector('${PROPS} select[aria-label="Formula size"]')?.value`,
        );
        await drag(handle.x, handle.y, handle.x + 80, handle.y + 40);
        const after = await nodeBox(f1);
        check(
          "a formula's corner handle scales the formula",
          after.w > before.w * 1.3 &&
            Math.abs(after.w / before.w - after.h / before.h) < 0.12,
          JSON.stringify({ before: before.w, after: after.w }),
        );
        check(
          "scaling changes the formula's size (not just its box)",
          (await page.evaluate(
            `document.querySelector('${PROPS} select[aria-label="Formula size"]')?.value`,
          )) !== sizeBefore,
        );
        await clearSelection();
      }

      // --- A rectangle with a label: colours.
      let boxId;
      {
        const before = await nodeIds("rect");
        await pickTool("Rectangle");
        const a = at(0.08, 0.62);
        await drag(a.x, a.y, a.x + svg.w * 0.16, a.y + svg.h * 0.1);
        boxId = (await nodeIds("rect")).find((id) => !before.includes(id));
        const bb = await nodeBox(boxId);
        await dbl(bb.cx, bb.cy);
        await waitFor(page, `document.querySelector("[data-label-editor]") !== null`);
        await insertText(page, "Box");
        await press(page, "Enter", { ctrl: true, wait: 300 });
        await tap(bb.cx, bb.cy);
        check(
          "the drawn rectangle is selected with its colour controls",
          await exists(page, `${PROPS} [data-colour-control="Stroke"]`),
        );
        // Custom stroke colour through the HSL picker's hex field.
        await click(page, `${PROPS} button[aria-label="Stroke custom colour"]`);
        check(
          "the + swatch opens a square HSL picker with a hex field",
          await page.evaluate(
            `(() => { const p = document.querySelector("[data-colour-picker]"); if (!p) return false; return getComputedStyle(p).borderRadius === "0px" && p.querySelectorAll('input[type="range"]').length === 3 && !!p.querySelector('input[aria-label="Stroke hex"]'); })()`,
          ),
        );
        await page.evaluate(`(() => {
          const i = document.querySelector('${PROPS} input[aria-label="Stroke hex"]');
          i.focus();
          i.select();
        })()`);
        await insertText(page, "#D95F02");
        await press(page, "Enter", { wait: 400 });
        check(
          "the picker sets the node's stroke to the exact hex (lowercase)",
          (await attr(boxId, "path", "stroke")) === "#d95f02",
          String(await attr(boxId, "path", "stroke")),
        );
        check(
          "custom swatches are marked and sit after the tokens",
          await page.evaluate(
            `(() => { const row = document.querySelector('${PROPS} [data-colour-control="Stroke"] [role="group"]'); const c = row.querySelector('button[data-custom="#d95f02"]'); if (!c) return false; const kids = [...row.children]; return c.classList.contains("mng-custom") && kids.indexOf(c) > kids.findIndex((k) => k.dataset.token === "ink") && getComputedStyle(c, "::after").content !== "none"; })()`,
          ),
        );
        check(
          "the recent colours row remembers it per user (localStorage, not the scene)",
          (
            await page.evaluate(
              `JSON.parse(localStorage.getItem("manang.figure.recent-colours") ?? "[]")`,
            )
          ).includes("#d95f02"),
        );
        // Nine more colours: the row keeps the latest eight, newest first.
        for (let i = 0; i < 9; i += 1) {
          const hex = `#${(0x102030 + i * 0x0a0b0c).toString(16)}`;
          await page.evaluate(`(() => {
            const i = document.querySelector('${PROPS} input[aria-label="Stroke hex"]');
            i.focus();
            i.select();
          })()`);
          await insertText(page, hex);
          await press(page, "Enter", { wait: 200 });
        }
        const recent = await page.evaluate(
          `JSON.parse(localStorage.getItem("manang.figure.recent-colours") ?? "[]")`,
        );
        check(
          "the recent row holds the last eight custom colours, newest first",
          recent.length === 8 &&
            recent[0] === `#${(0x102030 + 8 * 0x0a0b0c).toString(16)}`,
          JSON.stringify(recent),
        );
        // Set the stroke back to the first custom colour from the recent row.
        await click(
          page,
          `${PROPS} [data-colour-control="Stroke"] button[data-custom="${recent[7]}"]`,
        );
        check(
          "clicking a recent colour applies it",
          (await attr(boxId, "path", "stroke")) === recent[7],
        );
        // Text colour is separate from the stroke.
        const strokeNow = await attr(boxId, "path", "stroke");
        await click(page, `${PROPS} button[aria-label="Text blue"]`);
        const textFill = await attr(boxId, "text", "fill");
        check(
          "the label colour is its own setting, apart from the stroke",
          textFill !== null &&
            textFill !== strokeNow &&
            (await attr(boxId, "path", "stroke")) === strokeNow,
          `${textFill} vs ${strokeNow}`,
        );
        await clickText(
          page,
          `${PROPS} [data-colour-control="Text"] button`,
          "Follows stroke",
        );
        await sleep(250);
        check(
          '"Follows stroke" puts the text back on the stroke colour',
          (await attr(boxId, "text", "fill")) === (await attr(boxId, "path", "stroke")),
        );
        // Fill: a custom fill, a separate opacity, then none.
        await click(page, `${PROPS} button[aria-label="Fill blue"]`);
        check(
          "a hue fill paints as the tint",
          (await attr(boxId, "path", "fill-opacity")) === "0.2",
        );
        await page.evaluate(`(() => {
          const r = document.querySelector('${PROPS} input[aria-label="Fill opacity"]');
          r.value = "70";
          r.dispatchEvent(new Event("change", { bubbles: true }));
        })()`);
        await sleep(250);
        check(
          "fill opacity is separate from the overall opacity",
          (await attr(boxId, "path", "fill-opacity")) === "0.7" &&
            (await page.evaluate(
              `document.querySelector('${CANVAS} g[data-node-id="${boxId}"]')?.getAttribute("opacity")`,
            )) === null,
        );
        await click(page, `${PROPS} button[aria-label="Fill none"]`);
        check(
          "No fill removes the fill",
          (await attr(boxId, "path", "fill")) === "none",
        );
        await shoot(page, "17f-figure-colours");
      }

      // --- One colour for a multi-selection is one undo step.
      {
        const a = await nodeBox(boxId);
        const other = (await nodeIds("ellipse"))[0];
        const e = await nodeBox(other);
        await tap(a.cx, a.cy);
        await tap(e.cx, e.cy, { shift: true });
        const oldA = await attr(boxId, "path", "stroke");
        const oldE = await attr(other, "path", "stroke");
        await click(page, `${PROPS} button[aria-label="Stroke green"]`);
        const gA = await attr(boxId, "path", "stroke");
        const gE = await attr(other, "path", "stroke");
        check(
          "a colour applies to every selected node",
          gA === gE && gA !== oldA,
          `${gA} ${gE}`,
        );
        await press(page, "z", { ctrl: true, wait: 400 });
        check(
          "and one undo takes it back on all of them",
          (await attr(boxId, "path", "stroke")) === oldA &&
            (await attr(other, "path", "stroke")) === oldE,
        );
        await clearSelection();
      }

      // --- Canvas background: applies to the export; print export keeps working.
      {
        await click(page, `${PROPS} button[aria-label="Background paper"]`);
        await sleep(300);
        const pageFill = await page.evaluate(
          `document.querySelector('${CANVAS} rect[data-page]')?.getAttribute("fill")`,
        );
        const svgText = await exportSvg();
        const exportStroke = await attr(boxId, "path", "stroke");
        check(
          "the page shows its background and the export draws it behind everything",
          pageFill !== "none" &&
            /<rect x="[^"]*" y="[^"]*" width="[^"]*" height="[^"]*" fill="#ffffff"\/>/.test(
              svgText,
            ),
          String(pageFill),
        );
        check(
          "the export keeps the print theme, the custom stroke and the formulas",
          svgText.includes(`stroke="${exportStroke}"`) &&
            svgText.includes("data-approximate-math") &&
            !svgText.includes("var(--"),
        );
        await press(page, "z", { ctrl: true, wait: 400 });
        check(
          "undo removes the background again",
          (await page.evaluate(
            `document.querySelector('${CANVAS} rect[data-page]')?.getAttribute("fill")`,
          )) === "none" && !(await exportSvg()).includes('fill="#ffffff"/>'),
        );
      }

      // --- The diamond label no longer breaks mid-word.
      let dId;
      {
        dId = (await nodeIds("diamond"))[0];
        const d = await nodeBox(dId);
        await tap(d.cx, d.cy);
        await page.evaluate(`(() => {
          const i = document.querySelector('${PROPS} input[aria-label="Label"]');
          i.focus();
          i.value = "Validate input";
          i.dispatchEvent(new Event("change", { bubbles: true }));
        })()`);
        await sleep(300);
        const tspans = await page.evaluate(
          `[...document.querySelectorAll('${CANVAS} g[data-node-id="${dId}"] tspan')].map((t) => t.textContent)`,
        );
        check(
          "a diamond label breaks only at spaces",
          tspans.join(" ") === "Validate input",
          JSON.stringify(tspans),
        );
        await shoot(page, "17g-figure-diamond");
        await clearSelection();
      }

      // --- Resizing a multi-selection scales it (Shift keeps the proportions).
      {
        const ids = [boxId, (await nodeIds("ellipse"))[0]];
        const boxes0 = [await nodeBox(ids[0]), await nodeBox(ids[1])];
        await tap(boxes0[0].cx, boxes0[0].cy);
        await tap(boxes0[1].cx, boxes0[1].cy, { shift: true });
        const sel0 = await rect(page, `${CANVAS} [data-selection-bounds]`);
        const h = await rect(page, `${CANVAS} [data-multi-handle][data-handle="se"]`);
        await drag(h.x, h.y, h.x + 100, h.y + 20);
        const boxes1 = [await nodeBox(ids[0]), await nodeBox(ids[1])];
        const sx = boxes1[0].w / boxes0[0].w;
        const sx2 = boxes1[1].w / boxes0[1].w;
        const sy = boxes1[0].h / boxes0[0].h;
        check(
          "dragging a multi-selection handle scales every selected node",
          sx > 1.1 && Math.abs(sx - sx2) < 0.1 && sy >= 1 && Math.abs(sx - sy) > 0.02,
          JSON.stringify({ sx, sx2, sy }),
        );
        await press(page, "z", { ctrl: true, wait: 400 });
        const back = await nodeBox(ids[0]);
        check(
          "scaling is one undo step",
          Math.abs(back.w - boxes0[0].w) < 1.5 && Math.abs(back.h - boxes0[0].h) < 1.5,
          JSON.stringify({ back: back.w, was: boxes0[0].w }),
        );
        const sel1 = await rect(page, `${CANVAS} [data-selection-bounds]`);
        const h2 = await rect(page, `${CANVAS} [data-multi-handle][data-handle="se"]`);
        await shiftDrag(h2.x, h2.y, h2.x + 90, h2.y + 10);
        const boxes2 = [await nodeBox(ids[0])];
        const px = boxes2[0].w / boxes0[0].w;
        const py = boxes2[0].h / boxes0[0].h;
        check(
          "Shift keeps the proportions while scaling",
          px > 1.05 && Math.abs(px - py) < 0.05,
          JSON.stringify({ px, py, sel0, sel1 }),
        );
        await press(page, "z", { ctrl: true, wait: 400 });
        await clearSelection();
      }

      // --- Mermaid inserted from the UI lands in free space, selected and in view.
      {
        const before = await sceneNodes();
        const boundsOf = (n) => {
          const xs = n.points?.map((p) => p.x);
          const ys = n.points?.map((p) => p.y);
          return n.points === undefined
            ? { x: n.x, y: n.y, r: n.x + n.width, b: n.y + n.height }
            : {
                x: Math.min(...xs),
                y: Math.min(...ys),
                r: Math.max(...xs),
                b: Math.max(...ys),
              };
        };
        await page.evaluate(`(() => {
          const target = document.querySelector('${CANVAS}');
          target.focus();
          const data = new DataTransfer();
          data.setData("text/plain", "flowchart LR\\n  P[Plan] --> Q{Ready?}\\n  Q -->|yes| R[Ship]\\n");
          target.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
        })()`);
        await waitFor(page, `document.querySelector("[data-mermaid-prompt]") !== null`);
        await clickText(page, "[data-mermaid-prompt] button", "Insert as shapes");
        await waitFor(
          page,
          `${sceneCountExpr}.then((n) => n === ${before.length + 5})`,
        );
        await sleep(400);
        const after = await sceneNodes();
        const oldIds = new Set(before.map((n) => n.id));
        const added = after.filter((n) => !oldIds.has(n.id));
        const shapes = added.filter((n) => n.points === undefined);
        const overlap = shapes.some((n) =>
          before
            .filter((o) => o.kind !== "frame")
            .map(boundsOf)
            .some((o) => {
              const a = boundsOf(n);
              return a.x < o.r && o.x < a.r && a.y < o.b && o.y < a.b;
            }),
        );
        check(
          "Mermaid lands in free space, overlapping nothing",
          added.length === 5 && !overlap,
          JSON.stringify({ added: added.length, overlap }),
        );
        const view = await rect(page, CANVAS);
        const boxes = [];
        for (const n of shapes) boxes.push(await nodeBox(n.id));
        check(
          "the inserted Mermaid is selected and scrolled into view",
          (await page.evaluate(
            `document.querySelectorAll('${CANVAS} [data-selected]').length`,
          )) === added.length &&
            boxes.every(
              (n) =>
                n !== null &&
                n.x >= view.left - 2 &&
                n.right <= view.right + 2 &&
                n.y >= view.top - 2 &&
                n.bottom <= view.bottom + 2,
            ),
          JSON.stringify({ view, first: boxes[0] }),
        );
        await shoot(page, "17h-figure-mermaid-placed");
        await press(page, "z", { ctrl: true, wait: 400 });
        await clearSelection();
        await click(page, `${FIG_TOOLS} button[aria-label="Zoom to fit"]`);
      }

      // --- Both themes: the figure's own colours stay readable on the canvas.
      {
        const runCmd = async (query) => {
          await press(page, "k", { ctrl: true, wait: 300 });
          await insertText(page, query);
          await press(page, "Enter", { wait: 600 });
        };
        const lum = (rgb) => {
          const [r, g, b] = rgb.map((c) => {
            const s = c / 255;
            return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
          });
          return 0.2126 * r + 0.7152 * g + 0.0722 * b;
        };
        const ratio = (a, b) => {
          const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
          return (x + 0.05) / (y + 0.05);
        };
        const parse = (css) =>
          (css.match(/\d+(\.\d+)?/g) ?? []).slice(0, 3).map(Number);
        for (const theme of ["light", "dark"]) {
          await runCmd(`Switch to ${theme} theme`);
          await sleep(400);
          const colours = await page.evaluate(`(() => {
            const host = document.querySelector('${CANVAS}').parentElement;
            const bg = getComputedStyle(host.closest(".bg-bg") ?? document.body).backgroundColor;
            const g = document.querySelector('${CANVAS} g[data-node-id="${dId}"]');
            const toRgb = (c) => { const d = document.createElement("span"); d.style.color = c; document.body.appendChild(d); const v = getComputedStyle(d).color; d.remove(); return v; };
            return {
              bg,
              stroke: toRgb(g.querySelector("path").getAttribute("stroke")),
              text: toRgb(g.querySelector("text")?.getAttribute("fill") ?? "currentColor"),
            };
          })()`);
          const bg = parse(colours.bg);
          check(
            `${theme} theme: the figure's stroke has visible contrast against the canvas`,
            ratio(parse(colours.stroke), bg) >= 3,
            `${colours.stroke} on ${colours.bg}: ${ratio(parse(colours.stroke), bg).toFixed(2)}`,
          );
          check(
            `${theme} theme: label text reads against the canvas (4.5:1)`,
            ratio(parse(colours.text), bg) >= 4.5,
            `${colours.text} on ${colours.bg}: ${ratio(parse(colours.text), bg).toFixed(2)}`,
          );
          const inkCheck = await page.evaluate(`(() => {
            const toRgb = (c) => { const d = document.createElement("span"); d.style.color = c; document.body.appendChild(d); const v = getComputedStyle(d).color; d.remove(); return v; };
            const panel = document.querySelector('${PROPS}');
            return { panelBg: getComputedStyle(panel).backgroundColor, text: getComputedStyle(panel).color };
          })()`);
          check(
            `${theme} theme: the properties text reads on its panel`,
            ratio(parse(inkCheck.text), parse(inkCheck.panelBg)) >= 4.5,
            `${inkCheck.text} on ${inkCheck.panelBg}`,
          );
          await click(page, `${FIG_TOOLS} button[aria-label="Zoom to fit"]`);
          await shoot(page, `17i-figure-${theme}`);
        }
        await clearSelection();
        await runCmd("Switch to dark theme");
        await sleep(300);
        await clearSelection();
        await press(page, "s", { ctrl: true, wait: 500 });
        check(
          "Ctrl+S saves the figure with its formulas and colours",
          !(await dirtyTabs(page)).includes("figures/state_lattice.figure"),
        );
      }
    }

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
    await waitFor(page, `document.querySelector('[data-pdf-viewer] canvas') !== null`, {
      label: "the Typst pages",
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
    await page.evaluate(`document.querySelector('[data-pdf-viewer]').scrollTop = 1e6`);
    await press(page, "j", { ctrl: true, alt: true, wait: 500 });
    const markRect = await rect(page, "[data-preview-mark]");
    const region = await rect(page, "[data-pdf-viewer]");
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
    section("PDF viewer");
    // Several pages, a web link and the bibliography's citations: the Typst
    // preview is the same PDF viewer as LaTeX (ADR 0006, 2026-10-18).
    const viewerSel = "[data-pdf-viewer]";
    const viewerTop = () =>
      page.evaluate(`document.querySelector("${viewerSel}").scrollTop`);
    await focusEditor(page);
    await caretToEnd(page);
    {
      const para =
        "Entropy production in driven quantum systems is bounded below by the thermodynamic length of the protocol.";
      let appendix =
        '\n= Appendix\n\nSee #link("https://typst.app")[the Typst website] for details.\n\n';
      for (let i = 0; i < 24; i += 1) appendix += `${para} Paragraph ${i}.\n\n`;
      await insertText(page, appendix);
    }
    await expectEventually(
      page,
      "the Typst document shows all its pages in one scroll",
      `(() => { const v = document.querySelector("${viewerSel}"); const n = v?.querySelectorAll("[data-page]").length ?? 0; return n >= 3 && (${PAGE_BOX})?.of === n && v.scrollHeight > v.clientHeight * 2; })()`,
      { timeout: 10000 },
    );
    await page.evaluate(`document.querySelector("${viewerSel}").scrollTop = 0`);
    await sleep(600);
    await shoot(page, "25-pdf-pages");

    // Text selection: drag across the title on the text layer.
    const title = await page.evaluate(`(() => {
      const s = [...document.querySelectorAll('[data-page="1"] [data-text-layer] span')].find((e) => e.textContent.includes("Quantum thermodynamic"));
      if (!s) return null;
      const r = s.getBoundingClientRect();
      return { left: r.left, right: r.right, y: (r.top + r.bottom) / 2 };
    })()`);
    if (title !== null) {
      await mouse(page, "mouseMoved", title.left + 1, title.y);
      await mouse(page, "mousePressed", title.left + 1, title.y);
      for (let x = title.left + 1; x <= title.right - 1; x += 16) {
        await page.send("Input.dispatchMouseEvent", {
          type: "mouseMoved",
          x,
          y: title.y,
          button: "left",
          buttons: 1,
        });
      }
      await mouse(page, "mouseReleased", title.right - 1, title.y);
      await sleep(200);
    }
    const selected = await page.evaluate(`getSelection().toString()`);
    check(
      "dragging over the text layer selects the words",
      selected.includes("Quantum thermodynamic"),
      JSON.stringify(selected),
    );
    check(
      "the selection uses the accent selection tone, and the text stays invisible",
      await page.evaluate(`(() => {
        const s = document.querySelector('[data-page="1"] [data-text-layer] span');
        const sel = getComputedStyle(s, "::selection");
        return sel.backgroundColor.startsWith("color(srgb") || sel.backgroundColor.includes("198, 241, 53") ? getComputedStyle(s).color === "rgba(0, 0, 0, 0)" : false;
      })()`),
      await page.evaluate(
        `getComputedStyle(document.querySelector('[data-page="1"] [data-text-layer] span'), "::selection").backgroundColor`,
      ),
    );
    check(
      "copy puts the selected PDF text on the clipboard as plain text",
      await page.evaluate(`(() => {
        const data = new DataTransfer();
        const event = new ClipboardEvent("copy", { clipboardData: data, bubbles: true, cancelable: true });
        document.querySelector('[data-page="1"] [data-text-layer] span').dispatchEvent(event);
        return event.defaultPrevented && data.getData("text/plain").includes("Quantum thermodynamic");
      })()`),
    );
    check(
      "a selection is not a source jump: the caret stays in the editor where it was",
      /Ln \d+, Col \d+/.test(await statusText(page)) &&
        (await page.evaluate(
          `document.querySelector(".cm-line[data-source-flash]") === null`,
        )),
    );

    // Alt+double-click selects a word; plain double-click goes to the source.
    const word = await page.evaluate(`(() => {
      const s = [...document.querySelectorAll('[data-page="1"] [data-text-layer] span')].find((e) => e.textContent.startsWith("Monte Carlo"));
      if (!s) return null;
      const r = s.getBoundingClientRect();
      return { x: r.left + 12, y: (r.top + r.bottom) / 2 };
    })()`);
    if (word !== null) {
      for (const [type, count] of [
        ["mousePressed", 1],
        ["mouseReleased", 1],
        ["mousePressed", 2],
        ["mouseReleased", 2],
      ]) {
        await page.send("Input.dispatchMouseEvent", {
          type,
          x: word.x,
          y: word.y,
          button: "left",
          buttons: 1,
          clickCount: count,
          modifiers: 1,
        });
      }
      await sleep(200);
    }
    check(
      "Alt+double-click selects the word under the pointer",
      (await page.evaluate(`getSelection().toString().trim()`)) === "Monte",
      await page.evaluate(`getSelection().toString()`),
    );
    await press(page, "Home", { ctrl: true });
    if (word !== null) {
      for (const [type, count] of [
        ["mousePressed", 1],
        ["mouseReleased", 1],
        ["mousePressed", 2],
        ["mouseReleased", 2],
      ]) {
        await mouse(page, type, word.x, word.y, "left", count);
      }
    }
    await expectEventually(
      page,
      "double-click on the page goes to the source line (Overleaf's default)",
      `document.querySelector('[role="tab"][data-tab][aria-selected="true"]')?.dataset.tab === "sections/results.typ" && document.querySelector(".cm-line[data-source-flash]") !== null`,
      { timeout: 4000 },
    );
    await clickText(page, '[role="tab"]', "main.typ");
    await waitFor(page, `document.querySelector("${viewerSel} canvas") !== null`);

    // Find in PDF.
    await click(page, `${viewerSel} [data-page="1"]`);
    await press(page, "f", { ctrl: true, wait: 300 });
    check(
      "Ctrl+F in the preview opens its own find bar, focused",
      await page.evaluate(
        `document.activeElement?.closest("[data-pdf-find]") !== null && document.querySelector('[data-pdf-find]')?.getAttribute("role") === "search"`,
      ),
    );
    await insertText(page, "entropy production");
    await expectEventually(
      page,
      "find counts the matches across all pages: 1 of N",
      `/^1 of (2[5-9]|[3-9]\\d)$/.test(document.querySelector("[data-pdf-find-count]")?.textContent.trim() ?? "")`,
      {
        show: `document.querySelector("[data-pdf-find-count]")?.textContent`,
        timeout: 6000,
      },
    );
    check(
      "the current match is outlined in the accent, the others in a quiet tone",
      await page.evaluate(`(() => {
        const current = document.querySelectorAll(".mng-hit[data-current]");
        const other = document.querySelector(".mng-hit:not([data-current])");
        if (current.length < 1 || other === null) return false;
        const c = getComputedStyle(current[0]);
        return c.outlineColor === "rgb(198, 241, 53)" && c.outlineStyle === "solid" && getComputedStyle(other).outlineStyle === "none";
      })()`),
    );
    const firstHit = await page.evaluate(
      `document.querySelector(".mng-hit[data-current]")?.getBoundingClientRect().top ?? null`,
    );
    await press(page, "Enter", { wait: 300 });
    check(
      "Enter moves to the next match",
      (await page.evaluate(
        `document.querySelector("[data-pdf-find-count]")?.textContent.trim().startsWith("2 of")`,
      )) &&
        (await page.evaluate(
          `document.querySelector(".mng-hit[data-current]")?.getBoundingClientRect().top ?? null`,
        )) !== firstHit,
    );
    await press(page, "Enter", { shift: true, wait: 300 });
    check(
      "Shift+Enter moves back",
      await page.evaluate(
        `document.querySelector("[data-pdf-find-count]")?.textContent.trim().startsWith("1 of")`,
      ),
    );
    const total = () =>
      page.evaluate(
        `Number(/of (\\d+)/.exec(document.querySelector("[data-pdf-find-count]")?.textContent ?? "")?.[1] ?? 0)`,
      );
    const anyCase = await total();
    await click(page, 'button[aria-label="Match case"]');
    const exactCase = await total();
    check(
      "Match case narrows the search to the lower-case matches",
      exactCase >= 1 && exactCase < anyCase,
      `${anyCase} -> ${exactCase}`,
    );
    await click(page, 'button[aria-label="Match case"]');
    await shoot(page, "26-pdf-find");
    await press(page, "Escape", { wait: 200 });
    check(
      "Esc closes the find bar and clears the highlights",
      !(await exists(page, "[data-pdf-find]")) && !(await exists(page, ".mng-hit")),
    );

    // Edit > Find and Select all reach the PDF viewer while it has the focus.
    await click(page, `${viewerSel} [data-page="1"]`);
    await click(page, 'header [data-menu="edit"]');
    await click(page, 'header [data-command="edit.find"]');
    await expectEventually(
      page,
      "Edit > Find opens the PDF's find bar while the viewer has the focus",
      `!!document.querySelector("[data-pdf-find]")`,
    );
    await press(page, "Escape", { wait: 200 });
    await click(page, `${viewerSel} [data-page="1"]`);
    await click(page, 'header [data-menu="edit"]');
    await click(page, 'header [data-command="edit.select-all"]');
    check(
      "Edit > Select all selects the PDF's text",
      await page.evaluate(
        `(document.getSelection()?.toString() ?? "").includes("Quantum thermodynamic bounds")`,
      ),
      await page.evaluate(`(document.getSelection()?.toString() ?? "").slice(0, 80)`),
    );
    await page.evaluate(`document.getSelection()?.removeAllRanges()`);
    // The menu clicks moved the focus; the zoom checks below use the viewer's keys.
    await page.evaluate(
      `document.querySelector("${viewerSel}").focus({ preventScroll: true })`,
    );

    // Ctrl+wheel zooms at the pointer, without zooming the page itself.
    await page.evaluate(`document.querySelector("${viewerSel}").scrollTop = 300`);
    await sleep(300);
    const box1 = await rect(page, viewerSel);
    const pointer = { x: box1.left + box1.w * 0.6, y: box1.top + box1.h * 0.45 };
    const anchor = await page.evaluate(`(() => {
      const slot = document.elementFromPoint(${pointer.x}, ${pointer.y})?.closest("[data-index]");
      if (!slot) return null;
      const r = slot.getBoundingClientRect();
      return { index: slot.dataset.index, fx: (${pointer.x} - r.left) / r.width, fy: (${pointer.y} - r.top) / r.height, width: r.width };
    })()`);
    for (let i = 0; i < 4; i += 1) {
      await page.send("Input.dispatchMouseEvent", {
        type: "mouseWheel",
        x: pointer.x,
        y: pointer.y,
        deltaX: 0,
        deltaY: -100,
        modifiers: 2,
      });
      await sleep(30);
    }
    await sleep(700);
    const moved = await page.evaluate(`(() => {
      const slot = document.querySelector('[data-index="${anchor?.index}"]');
      if (!slot) return null;
      const r = slot.getBoundingClientRect();
      return { x: r.left + ${anchor?.fx} * r.width, y: r.top + ${anchor?.fy} * r.height, width: r.width };
    })()`);
    check(
      "Ctrl+wheel zooms in, keeping the point under the pointer within 3px",
      anchor !== null &&
        moved !== null &&
        moved.width > anchor.width * 1.3 &&
        Math.abs(moved.x - pointer.x) <= 3 &&
        Math.abs(moved.y - pointer.y) <= 3,
      JSON.stringify({ pointer, moved }),
    );
    check(
      "the browser's own zoom is untouched",
      await page.evaluate(
        `window.visualViewport.scale === 1 && document.documentElement.style.zoom === ""`,
      ),
    );
    await expectEventually(
      page,
      "after the zoom settles the visible pages are redrawn sharply",
      `(() => { const c = document.querySelector('${viewerSel} [data-page] canvas'); if (!c) return false; const r = c.getBoundingClientRect(); return c.width >= Math.floor(r.width * devicePixelRatio) - 2; })()`,
    );
    await shoot(page, "27-pdf-wheel-zoom");

    // Presets: a menu that the keyboard can use.
    await press(page, "0", { ctrl: true, wait: 300 });
    const fitWidth = await page.evaluate(
      `document.querySelector('${viewerSel} [data-page="1"]').getBoundingClientRect().width`,
    );
    await page.evaluate(`document.querySelector("[data-zoom-menu]").focus()`);
    await press(page, "ArrowDown", { wait: 200 });
    check(
      "the zoom value opens a menu of presets from the keyboard",
      await page.evaluate(`(() => {
        const items = [...document.querySelectorAll('[role="menu"] [role^="menuitem"]')].map((b) => b.textContent.trim());
        return ["Fit width", "Fit page", "50%", "75%", "100%", "125%", "150%", "200%", "300%", "400%"].every((l) => items.some((t) => t.startsWith(l))) && document.activeElement?.getAttribute("role")?.startsWith("menuitem");
      })()`),
    );
    check(
      "the current zoom is the checked entry",
      await page.evaluate(
        `[...document.querySelectorAll('[role="menuitemradio"][aria-checked="true"]')].map((b) => b.textContent.trim()).join() .startsWith("Fit width")`,
      ),
    );
    await page.evaluate(
      `[...document.querySelectorAll('[role="menu"] [role^="menuitem"]')].find((b) => b.textContent.trim().startsWith("200%")).click()`,
    );
    await sleep(400);
    const at200 = await page.evaluate(
      `document.querySelector('${viewerSel} [data-page="1"]').getBoundingClientRect().width`,
    );
    check(
      "the 200% preset shows the page at twice its printed size",
      Math.abs(at200 - 595.28 * (96 / 72) * 2) < 3 && at200 !== fitWidth,
      `${fitWidth} -> ${at200}`,
    );
    await press(page, "-", { ctrl: true, wait: 300 });
    check(
      "Ctrl+- steps down a preset",
      (await page.evaluate(
        `document.querySelector("[data-zoom-menu]").textContent.trim()`,
      )) === "150%",
    );
    await press(page, "=", { ctrl: true, wait: 300 });
    await press(page, "=", { ctrl: true, wait: 300 });
    await press(page, "=", { ctrl: true, wait: 600 });
    check(
      "Ctrl+= steps up to 400% at most",
      (await page.evaluate(
        `document.querySelector("[data-zoom-menu]").textContent.trim()`,
      )) === "400%",
    );
    const memory = await page.evaluate(`(() => ({
      canvases: document.querySelectorAll("${viewerSel} canvas").length,
      pixels: [...document.querySelectorAll("${viewerSel} canvas")].reduce((n, c) => n + c.width * c.height, 0),
      heapMB: performance.memory ? Math.round(performance.memory.usedJSHeapSize / 1048576) : null,
      pages: document.querySelectorAll("${viewerSel} [data-page]").length,
    }))()`);
    check(
      "at 400% only the pages near the viewport own a canvas",
      memory.canvases <= 3 && memory.canvases < memory.pages,
      JSON.stringify(memory),
    );
    await press(page, "0", { ctrl: true, wait: 400 });
    check(
      "Ctrl+0 fits the width again",
      Math.abs(
        (await page.evaluate(
          `document.querySelector('${viewerSel} [data-page="1"]').getBoundingClientRect().width`,
        )) - fitWidth,
      ) < 2,
    );

    // Links: a citation scrolls to the bibliography; a web link opens the
    // system browser (here: window.open), never the webview.
    await page.evaluate(`document.querySelector("${viewerSel}").scrollTop = 0`);
    await sleep(300);
    const cite = await rect(
      page,
      `${viewerSel} [data-page="1"] a[data-pdf-link="internal"]`,
    );
    if (cite !== null) {
      await mouse(page, "mouseMoved", cite.x, cite.y);
      check(
        "links show a pointer",
        await page.evaluate(
          `getComputedStyle(document.querySelector('${viewerSel} a[data-pdf-link]')).cursor === "pointer"`,
        ),
      );
      await mouse(page, "mousePressed", cite.x, cite.y);
      await mouse(page, "mouseReleased", cite.x, cite.y);
    }
    await expectEventually(
      page,
      "a citation link scrolls to the References heading",
      `(() => { const h = [...document.querySelectorAll("${viewerSel} [data-text-layer] span")].find((s) => s.textContent === "References"); if (!h) return false; const top = h.getBoundingClientRect().top - document.querySelector("${viewerSel}").getBoundingClientRect().top; return document.querySelector("${viewerSel}").scrollTop > 0 && top >= 0 && top < 80; })()`,
      { timeout: 4000 },
    );
    await page.evaluate(
      `window.__opened = []; window.open = (url) => { window.__opened.push(url); return null; }`,
    );
    const web = await rect(page, `${viewerSel} a[data-pdf-link="external"]`);
    if (web !== null) {
      await page.evaluate(
        `document.querySelector('${viewerSel} a[data-pdf-link="external"]').scrollIntoView({ block: "center" })`,
      );
      const webNow = await rect(page, `${viewerSel} a[data-pdf-link="external"]`);
      await mouse(page, "mouseMoved", webNow.x, webNow.y);
      await mouse(page, "mousePressed", webNow.x, webNow.y);
      await mouse(page, "mouseReleased", webNow.x, webNow.y);
      await sleep(300);
    }
    check(
      "a web link opens outside the app and the webview stays put",
      (await page.evaluate(`JSON.stringify(window.__opened)`)) ===
        '["https://typst.app/"]' && (await page.evaluate(`location.pathname`)) === "/",
      await page.evaluate(`JSON.stringify(window.__opened)`),
    );

    // The page box, keys, outline and dark pages.
    await click(page, "[data-page-input]");
    await page.evaluate(`document.querySelector("[data-page-input]").select()`);
    await insertText(page, "3");
    await press(page, "Enter", { wait: 400 });
    check(
      "typing a page number and Enter jumps to that page",
      (await page.evaluate(PAGE_BOX))?.page === 3 &&
        (await page.evaluate(
          `Math.abs(document.querySelector('${viewerSel} [data-page="3"]').getBoundingClientRect().top - document.querySelector("${viewerSel}").getBoundingClientRect().top - 16) < 4`,
        )),
      JSON.stringify(await page.evaluate(PAGE_BOX)),
    );
    await page.evaluate(`document.querySelector("${viewerSel}").focus()`);
    await press(page, "Home", { wait: 300 });
    check("Home goes to the first page", (await viewerTop()) === 0);
    await press(page, "PageDown", { wait: 300 });
    check(
      "PageDown goes to the next page",
      (await page.evaluate(PAGE_BOX))?.page === 2,
    );
    await press(page, "End", { wait: 300 });
    check(
      "End goes to the last page",
      (await page.evaluate(PAGE_BOX))?.page === (await page.evaluate(PAGE_BOX))?.of,
    );
    await click(page, 'button[aria-label="PDF outline"]');
    check(
      "the outline lists the document's headings",
      await page.evaluate(`(() => {
        const t = document.querySelector("[data-pdf-outline]")?.innerText ?? "";
        return t.includes("Quantum thermodynamic bounds") && t.includes("References") && t.includes("Appendix");
      })()`),
    );
    await shoot(page, "28-pdf-outline");
    await clickText(page, "[data-pdf-outline] button", "Quantum thermodynamic bounds");
    await sleep(300);
    check("an outline entry scrolls to its heading", (await viewerTop()) < 40);
    await click(page, 'button[aria-label="PDF outline"]');
    check("the outline toggles off", !(await exists(page, "[data-pdf-outline]")));
    // In this split the pane is narrow enough that the toggle gives way.
    await page.evaluate(
      `document.querySelector('button[aria-label="Dark pages"]').click()`,
    );
    await sleep(200);
    check(
      "Dark pages inverts the page pictures only, keeping hues",
      await page.evaluate(`(() => {
        const c = getComputedStyle(document.querySelector("${viewerSel} canvas")).filter;
        const t = getComputedStyle(document.querySelector("${viewerSel} [data-text-layer]")).filter;
        return c === "invert(1) hue-rotate(180deg)" && t === "none";
      })()`),
    );
    await shoot(page, "29-pdf-dark");
    // In this split the pane is narrow enough that the toggle gives way.
    await page.evaluate(
      `document.querySelector('button[aria-label="Dark pages"]').click()`,
    );
    await sleep(200);
    check(
      "Dark pages toggles back off",
      await page.evaluate(
        `getComputedStyle(document.querySelector("${viewerSel} canvas")).filter === "none"`,
      ),
    );

    // The reading position survives switching to another document and back.
    await page.evaluate(`document.querySelector("${viewerSel}").scrollTop = 700`);
    await sleep(400);
    const readingAt = await viewerTop();
    await goToFile("notes.md");
    await waitFor(
      page,
      `!!document.querySelector('article[aria-label="Markdown preview"]')`,
    );
    await clickText(page, '[role="tab"]', "main.typ");
    await waitFor(page, `document.querySelector("${viewerSel} canvas") !== null`);
    await sleep(500);
    check(
      "switching tabs and back keeps the place in the PDF",
      Math.abs((await viewerTop()) - readingAt) < 4,
      `${readingAt} -> ${await viewerTop()}`,
    );

    // At 1024px with the side panel open nothing in the header is cut off.
    await resize(page, 1024, 700);
    await sleep(300);
    check(
      "at 1024px with the side panel open every header control is whole and clickable",
      await page.evaluate(`(() => {
        if (!document.querySelector("#side-panel")) return false;
        const s = document.querySelector('section[aria-label="Preview"]');
        const bar = s.querySelector('[role="toolbar"]');
        const sr = s.getBoundingClientRect();
        const shown = [...bar.querySelectorAll("button, input")].filter((b) => b.getBoundingClientRect().width > 0);
        return bar.scrollWidth <= bar.clientWidth && shown.length >= 6 && shown.every((b) => { const r = b.getBoundingClientRect(); const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return r.left >= sr.left && r.right <= sr.right && (hit === b || b.contains(hit)); });
      })()`),
      await page.evaluate(
        `[...document.querySelectorAll('section[aria-label="Preview"] [role="toolbar"] button, section[aria-label="Preview"] [role="toolbar"] input')].filter((b) => b.getBoundingClientRect().width > 0).map((b) => b.getAttribute("aria-label")).join("|")`,
      ),
    );
    await shoot(page, "30-pdf-header-1024");
    await resize(page, 1280, 820);
    await sleep(300);

    // Put main.typ back as it was.
    await focusEditor(page);
    for (let i = 0; i < 6 && (await dirtyTabs(page)).includes("main.typ"); i += 1)
      await press(page, "z", { ctrl: true, wait: 400 });
    await noStrayEdits(page, "after the PDF viewer");

    /* ------------------------------------------------------------------ */
    section("Diagrams");
    const diagramImg = `section[aria-label="Preview"] img[data-diagram]`;
    const runCommand = async (query) => {
      await press(page, "k", { ctrl: true, wait: 300 });
      await insertText(page, query);
      await press(page, "Enter", { wait: 500 });
    };
    await press(page, "p", { ctrl: true, wait: 300 });
    await insertText(page, "flow.mmd");
    await press(page, "Enter", { wait: 600 });
    check(
      "Ctrl+P opens figures/flow.mmd",
      (await activeTab(page)) === "figures/flow.mmd",
    );
    await expectEventually(
      page,
      "the diagram is drawn in the preview, with a nonzero size",
      `(() => { const i = document.querySelector(${JSON.stringify(diagramImg)}); if (!i || !i.complete || i.naturalWidth === 0) return false; const r = i.getBoundingClientRect(); return r.width > 0 && r.height > 0; })()`,
    );
    check(
      "the header names the diagram language and Export PDF is disabled",
      await page.evaluate(
        `(() => { const s = document.querySelector('section[aria-label="Preview"]'); const e = s.querySelector('button[aria-label="Export PDF"]'); return s.querySelector('[role="toolbar"]').innerText.includes("Mermaid") && e.disabled && e.title.includes("document that includes"); })()`,
      ),
    );
    check(
      "the Mermaid source is highlighted",
      await page.evaluate(`document.querySelectorAll(".cm-content span").length > 3`),
    );
    await shoot(page, "diagram-preview");
    const goodSrc = await page.evaluate(
      `document.querySelector(${JSON.stringify(diagramImg)}).src`,
    );

    await focusEditor(page);
    await caretToEnd(page);
    await insertText(page, "  D -->>> E");
    await expectEventually(
      page,
      "a diagram error keeps the old picture, dimmed, and names the line",
      `(() => { const i = document.querySelector(${JSON.stringify(diagramImg)}); const e = document.querySelector("[data-diagram-error]"); return !!i && i.hasAttribute("data-stale") && i.src === ${JSON.stringify(goodSrc)} && Number(getComputedStyle(i).opacity) < 1 && !!e && e.innerText.includes("Line 6"); })()`,
    );
    await expectEventually(
      page,
      "the gutter marks the broken line",
      `document.querySelectorAll(".manang-problem-error").length >= 1`,
    );
    await press(page, "m", { ctrl: true, shift: true, wait: 300 });
    await expectEventually(
      page,
      "Problems lists the diagram error on its line",
      `(() => { const t = document.querySelector("#bottom-panel")?.innerText ?? ""; return t.includes("figures/flow.mmd") && t.includes("unexpected token") && /Ln 6, Col 5/.test(t); })()`,
      { show: `document.querySelector("#bottom-panel")?.innerText` },
    );
    await shoot(page, "diagram-error");
    await focusEditor(page);
    await press(page, "z", { ctrl: true, wait: 300 });
    await expectEventually(
      page,
      "fixing the source clears the error and redraws",
      `(() => { const i = document.querySelector(${JSON.stringify(diagramImg)}); return !!i && !i.hasAttribute("data-stale") && document.querySelector("[data-diagram-error]") === null && !(document.querySelector("#bottom-panel")?.innerText ?? "").includes("unexpected token") && document.querySelectorAll(".manang-problem-error").length === 0; })()`,
    );
    await press(page, "j", { ctrl: true, wait: 200 });

    const darkSrc = await page.evaluate(
      `document.querySelector(${JSON.stringify(diagramImg)}).src`,
    );
    await runCommand("Switch to light theme");
    await expectEventually(
      page,
      "switching the app theme redraws the diagram in that theme",
      `(() => { const i = document.querySelector(${JSON.stringify(diagramImg)}); return !!i && i.dataset.theme === "light" && i.src !== ${JSON.stringify(darkSrc)}; })()`,
    );
    await shoot(page, "diagram-light");
    await runCommand("Switch to dark theme");
    await expectEventually(
      page,
      "and back to dark",
      `document.querySelector(${JSON.stringify(diagramImg)})?.dataset.theme === "dark"`,
    );

    // Saving writes the print-ready picture next to the source.
    const showFiles = async () => {
      // The Files button toggles the side panel when Files is already shown.
      if (!(await exists(page, '[role="treeitem"][data-path="main.typ"]'))) {
        await click(
          page,
          'nav[aria-label="Workbench views"] button[aria-label="Files"]',
        );
      }
      await waitFor(
        page,
        `!!document.querySelector('[role="treeitem"][data-path="main.typ"]')`,
        {
          label: "Files is shown",
        },
      );
      await page.evaluate(
        `(() => { const d = document.querySelector('[role="treeitem"][data-path="figures"]'); if (d && d.getAttribute("aria-expanded") === "false") d.click(); })()`,
      );
      await sleep(300);
    };
    await showFiles();
    check(
      "flow.mmd has no picture next to it before the first save",
      (await exists(page, '[role="treeitem"][data-path="figures/flow.mmd"]')) &&
        !(await exists(page, '[role="treeitem"][data-path="figures/flow.svg"]')),
    );
    await focusEditor(page);
    await caretToEnd(page);
    await insertText(page, "  D --> E[Report]\n");
    await press(page, "s", { ctrl: true, wait: 600 });
    await expectEventually(
      page,
      "saving flow.mmd makes figures/flow.svg appear in Files",
      `!!document.querySelector('[role="treeitem"][data-path="figures/flow.svg"]')`,
    );
    await shoot(page, "diagram-saved-svg");

    await runCommand("Copy include line");
    await expectEventually(
      page,
      "Copy include line gives the Typst include for the main document",
      `(() => { const t = document.querySelector('footer[aria-label="Status"]').innerText; return t.includes('#figure(image("figures/flow.svg"))') && !t.includes("not write"); })()`,
      { show: `document.querySelector('footer[aria-label="Status"]').innerText` },
    );

    await runCommand("New diagram");
    await expectEventually(
      page,
      "New diagram asks for a name, with focus in the field",
      `document.activeElement === document.querySelector('[role="alertdialog"] input')`,
    );
    check(
      "New diagram has the kind as one single-select and one action row (Cancel, Create)",
      await page.evaluate(`(() => {
        const box = document.querySelector('[role="alertdialog"]');
        const radios = [...box.querySelectorAll('[role="radiogroup"] [role="radio"]')];
        const actions = [...box.querySelectorAll('button:not([role="radio"])')];
        const tops = (els) => new Set(els.map((e) => Math.round(e.getBoundingClientRect().top)));
        return radios.length === 4 && tops(radios).size === 1 &&
          radios.filter((r) => r.getAttribute("aria-checked") === "true").length === 1 &&
          actions.map((b) => b.textContent.trim()).join() === "Cancel,Create" && tops(actions).size === 1;
      })()`),
    );
    await shoot(page, "diagram-new");
    await insertText(page, "setup");
    await clickText(page, '[role="alertdialog"] [role="radio"]', "Pikchr");
    check(
      "picking a kind selects it",
      await page.evaluate(
        `[...document.querySelectorAll('[role="alertdialog"] [role="radio"][aria-checked="true"]')].map((r) => r.textContent.trim()).join() === "Pikchr"`,
      ),
    );
    await clickText(page, '[role="alertdialog"] button', "Create");
    await expectEventually(
      page,
      "it writes figures/setup.pikchr and opens it with a preview",
      `document.querySelector('[role="tab"][data-tab][aria-selected="true"]')?.dataset.tab === "figures/setup.pikchr" && document.querySelector(${JSON.stringify(diagramImg)}) !== null`,
    );
    await runCommand("New diagram");
    await waitFor(
      page,
      `document.querySelector('[role="alertdialog"] input') !== null`,
    );
    await insertText(page, "setup");
    await clickText(page, '[role="alertdialog"] [role="radio"]', "Pikchr");
    await clickText(page, '[role="alertdialog"] button', "Create");
    await expectEventually(
      page,
      "a taken name is never overwritten: Manang asks",
      `(document.querySelector('[role="alertdialog"]')?.innerText ?? "").includes("already exists")`,
    );
    await press(page, "Escape", { wait: 300 });
    await noStrayEdits(page, "after the diagram checks");
    for (const name of ["setup.pikchr", "flow.mmd"]) {
      await clickText(page, '[role="tab"]', name);
      await press(page, "w", { ctrl: true, synthetic: true, wait: 300 });
    }

    // Diagram fences in Markdown: pictures in the preview, errors in Problems.
    await showFiles();
    await click(page, '[role="treeitem"][data-path="notes.md"]');
    await waitFor(
      page,
      `document.querySelector('[role="tab"][data-tab][aria-selected="true"]')?.dataset.tab === "notes.md"`,
      {
        label: "notes.md open",
      },
    );
    await focusEditor(page);
    await caretToEnd(page);
    await insertText(
      page,
      "\n```mermaid\nflowchart LR\n  A[Draft] --> B[Paper]\n```\n",
    );
    const fenceImg = `article[aria-label="Markdown preview"] figure.mng-diagram img[data-diagram-fence]`;
    await expectEventually(
      page,
      "a mermaid fence in Markdown shows as an image in the preview",
      `(() => { const i = document.querySelector(${JSON.stringify(fenceImg)}); if (!i || !i.complete || i.naturalWidth === 0) return false; return i.src.startsWith("blob:") && i.getBoundingClientRect().width > 0 && !document.querySelector('article[aria-label="Markdown preview"] svg'); })()`,
    );
    await shoot(page, "diagram-markdown-fence");
    await insertText(page, "\n```mermaid\nflowchart TD\n  A -->>> B\n```\n");
    // The broken fence opens on the last line that reads ```mermaid; its
    // error is two lines further down.
    const fenceLine = await page.evaluate(
      `(() => { const lines = [...document.querySelectorAll(".cm-line")].map((l) => l.textContent); return lines.lastIndexOf("\`\`\`mermaid") + 1; })()`,
    );
    await expectEventually(
      page,
      "a broken fence shows a quiet error box in the preview",
      `(() => { const e = document.querySelector('article[aria-label="Markdown preview"] figure[data-diagram-error]'); return !!e && e.innerText.includes("line ${fenceLine}:") && e.innerText.includes("unexpected token"); })()`,
      {
        show: `document.querySelector('article[aria-label="Markdown preview"]')?.innerHTML.slice(0, 600)`,
      },
    );
    await runCommand("Show problems");
    await expectEventually(
      page,
      "Problems lists the broken fence at its line in notes.md",
      `(() => { const t = document.querySelector("#bottom-panel")?.innerText ?? ""; return t.includes("notes.md") && t.includes("unexpected token") && /Ln ${fenceLine + 2}, Col 5/.test(t); })()`,
      { show: `document.querySelector("#bottom-panel")?.innerText` },
    );
    await shoot(page, "diagram-markdown-error");
    await press(page, "j", { ctrl: true, wait: 200 });
    // Leave notes.md as it was.
    await click(page, '[role="tab"][data-tab="notes.md"]', "middle");
    if (await exists(page, '[role="alertdialog"]')) {
      await clickText(page, '[role="alertdialog"] button', "Don't save");
      await sleep(300);
    }
    await noStrayEdits(page, "after the Markdown fences");

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
      "the workbench top bar has no buttons but Home, the menus and the window controls",
      await page.evaluate(
        `[...document.querySelectorAll("header button")].every((b) => b.getAttribute("aria-label") === "Home" || b.closest("[data-menu-root]") !== null || ${WINDOW_BUTTONS}.includes(b.getAttribute("aria-label")))`,
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
    section("Menu bar");
    const menuOpen = (label) =>
      page.evaluate(
        `document.querySelector('header [role="menu"][aria-label="${label}"]') !== null`,
      );
    const anyMenuOpen = () =>
      page.evaluate(`document.querySelector('header [role="menu"]') !== null`);
    const panelOpen = () => exists(page, "#bottom-panel");
    check(
      "the top bar shows six menus: File, Edit, View, Go, Build, Help",
      (await page.evaluate(
        `[...document.querySelectorAll('header [role="menubar"] [data-menu-bar]')].map((b) => b.textContent.trim()).join()`,
      )) === "File,Edit,View,Go,Build,Help",
    );
    check(
      "the menus sit after the mark, at the start of the free space",
      await page.evaluate(
        `(() => { const home = document.querySelector('header button[aria-label="Home"]').getBoundingClientRect(); const bar = document.querySelector('header [role="menubar"]').getBoundingClientRect(); const free = document.querySelector('header [data-menu-root]').parentElement.getBoundingClientRect(); return home.right <= bar.left && Math.abs(bar.left - free.left) < 2; })()`,
      ),
    );
    check(
      "menu buttons carry roles, popup state, Alt shortcuts and a roving tabindex",
      await page.evaluate(
        `(() => { const b = [...document.querySelectorAll('header [data-menu-bar]')]; return b.every((x) => x.getAttribute("role") === "menuitem" && x.getAttribute("aria-haspopup") === "menu" && x.getAttribute("aria-expanded") === "false" && /^Alt\\+[A-Z]$/.test(x.getAttribute("aria-keyshortcuts"))) && b.filter((x) => x.tabIndex === 0).length === 1; })()`,
      ),
    );
    check(
      "the bar's empty areas still drag the window; the menus do not",
      await page.evaluate(
        `(() => { const h = document.querySelector("header"); const free = h.querySelector("[data-menu-root]").parentElement; const bar = h.querySelector('[role="menubar"]').getBoundingClientRect(); return h.hasAttribute("data-tauri-drag-region") && free.hasAttribute("data-tauri-drag-region") && free.getBoundingClientRect().right - bar.right > 200 && [...h.querySelectorAll("[data-menu-root], [data-menu-root] *")].every((e) => !e.hasAttribute("data-tauri-drag-region")); })()`,
      ),
    );
    await focusEditor(page);
    await press(page, "f", { alt: true, wait: 300 });
    check("Alt+F opens File", await menuOpen("File"));
    check(
      "the mnemonics are underlined while the menus are in keyboard use",
      (await page.evaluate(
        `[...document.querySelectorAll('header [data-menu-bar] .underline')].map((u) => u.textContent).join("")`,
      )) === "FEVGBH",
    );
    check(
      "File lists its groups with separators and shows shortcuts in DM Mono",
      await page.evaluate(
        `(() => { const m = document.querySelector('header [role="menu"][aria-label="File"]'); const save = m.querySelector('[data-command="file.save"]'); const key = save?.querySelector(".font-mono"); return m.querySelectorAll('[role="separator"]').length >= 4 && key?.textContent === "Ctrl+S" && save.getAttribute("aria-keyshortcuts") === "Control+S" && getComputedStyle(key).fontFamily.includes("DM Mono"); })()`,
      ),
    );
    check(
      "the first enabled item has focus, marked with the accent",
      await page.evaluate(
        `(() => { const a = document.activeElement; return a?.getAttribute("role") === "menuitem" && a.dataset.menuItem === "0" && a.querySelector(".bg-accent") !== null; })()`,
      ),
    );
    check(
      "the open menu is a flat panel: hairline border, no radius, no shadow",
      await page.evaluate(
        `(() => { const m = getComputedStyle(document.querySelector('header [role="menu"]')); return m.borderTopWidth === "1px" && m.borderTopLeftRadius === "0px" && m.boxShadow === "none"; })()`,
      ),
    );
    await shoot(page, "18f-menu-file");
    await press(page, "ArrowRight", { wait: 150 });
    await press(page, "ArrowRight", { wait: 200 });
    check("arrow keys move from File to View, which opens", await menuOpen("View"));
    const itemBefore = await page.evaluate(`document.activeElement?.dataset.menuItem`);
    await press(page, "ArrowDown", { wait: 200 });
    check(
      "ArrowDown moves down View's items",
      await page.evaluate(
        `(() => { const a = document.activeElement; return a?.closest('[role="menu"]')?.getAttribute("aria-label") === "View" && a.dataset.menuItem === String(Number(${JSON.stringify(itemBefore)}) + 1); })()`,
      ),
    );
    await press(page, "Escape", { wait: 300 });
    check(
      "Escape closes the menu and gives focus back to the editor",
      !(await anyMenuOpen()) &&
        (await page.evaluate(`document.activeElement?.closest(".cm-editor") !== null`)),
    );
    await press(page, "F10", { wait: 200 });
    check(
      "F10 focuses the menu bar without opening a menu",
      !(await anyMenuOpen()) &&
        (await page.evaluate(`document.activeElement?.dataset.menuBar === "0"`)),
    );
    await press(page, "Escape", { wait: 200 });

    const panelBefore = await panelOpen();
    await click(page, 'header [data-menu="view"]');
    check("clicking View opens it", await menuOpen("View"));
    await mouse(
      page,
      "mouseMoved",
      ...(await rect(page, 'header [data-menu="go"]').then((r) => [r.x, r.y])),
    );
    await sleep(150);
    check("hovering Go while View is open switches to Go", await menuOpen("Go"));
    await mouse(
      page,
      "mouseMoved",
      ...(await rect(page, 'header [data-menu="view"]').then((r) => [r.x, r.y])),
    );
    await sleep(150);
    await click(page, 'header [role="menu"] [data-command="view.toggle-panel"]');
    check(
      "View > Toggle panel runs its command and closes the menu",
      (await panelOpen()) !== panelBefore && !(await anyMenuOpen()),
    );
    await press(page, "j", { ctrl: true, wait: 300 });
    check("Ctrl+J, its shortcut, toggles it back", (await panelOpen()) === panelBefore);

    await click(page, 'header [data-menu="file"]');
    check(
      "a disabled item is marked disabled (File > Quit in a browser)",
      await page.evaluate(
        `document.querySelector('header [data-command="app.quit"]')?.getAttribute("aria-disabled") === "true"`,
      ),
    );
    await click(page, 'header [data-command="app.quit"]');
    check(
      "clicking a disabled item does nothing; the menu stays open",
      await menuOpen("File"),
    );
    await mouse(page, "mousePressed", 700, 400);
    await mouse(page, "mouseReleased", 700, 400);
    await sleep(200);
    check("a click outside closes the menu", !(await anyMenuOpen()));

    await click(page, 'header [data-menu="edit"]');
    check(
      "Edit lists Undo, Redo, Cut, Copy, Paste, Select all, Find, Replace",
      (await page.evaluate(
        `[...document.querySelectorAll('header [role="menu"][aria-label="Edit"] [role="menuitem"]')].slice(0, 8).map((i) => i.querySelector("span.truncate").textContent).join()`,
      )) === "Undo,Redo,Cut,Copy,Paste,Select all,Find,Replace",
    );
    await click(page, 'header [data-command="edit.find"]');
    await expectEventually(
      page,
      "Edit > Find opens the editor's find panel (the focused surface)",
      `document.querySelector(".cm-editor .cm-search") !== null`,
      { timeout: 3000 },
    );
    await press(page, "Escape", { wait: 300 });
    await noStrayEdits(page, "menu bar");

    // A shortcut changed in Settings applies at once, here and in the menus.
    await click(page, 'header [data-menu="help"]');
    await click(page, 'header [data-command="help.keybindings"]');
    await waitFor(
      page,
      `document.querySelector('button[aria-label="Change shortcut for view.toggle-panel"]') !== null`,
      { timeout: 8000 },
    );
    await page.evaluate(
      `document.querySelector('button[aria-label="Change shortcut for view.toggle-panel"]').scrollIntoView({ block: "center" })`,
    );
    await sleep(150);
    await click(page, 'button[aria-label="Change shortcut for view.toggle-panel"]');
    await press(page, "y", { ctrl: true, alt: true, wait: 200 });
    await press(page, "Enter", { wait: 600 });
    await shoot(page, "18g-keybinding-changed");
    await press(page, "Escape", { wait: 400 });
    const panelNow = await panelOpen();
    // Synthetic: unbound, Ctrl+J would reach Chromium, which opens Downloads.
    await press(page, "j", { ctrl: true, wait: 300, synthetic: true });
    check(
      "after rebinding, the old shortcut Ctrl+J does nothing",
      (await panelOpen()) === panelNow,
    );
    await press(page, "y", { ctrl: true, alt: true, wait: 300 });
    check(
      "the shortcut changed in Settings > Keyboard shortcuts runs the command",
      (await panelOpen()) !== panelNow,
    );
    await click(page, 'header [data-menu="view"]');
    check(
      "the menu shows the new shortcut",
      (await page.evaluate(
        `document.querySelector('header [data-command="view.toggle-panel"] .font-mono')?.textContent`,
      )) === "Ctrl+Alt+Y",
    );
    await press(page, "Escape", { wait: 200 });
    await press(page, "y", { ctrl: true, alt: true, wait: 300 });
    // Put the default back for the sections that follow.
    await click(page, 'header [data-menu="help"]');
    await click(page, 'header [data-command="help.keybindings"]');
    await waitFor(
      page,
      `document.querySelector('button[aria-label="Reset shortcut for view.toggle-panel"]') !== null`,
    );
    await page.evaluate(
      `document.querySelector('button[aria-label="Reset shortcut for view.toggle-panel"]').scrollIntoView({ block: "center" })`,
    );
    await sleep(150);
    await click(page, 'button[aria-label="Reset shortcut for view.toggle-panel"]');
    await sleep(400);
    await press(page, "Escape", { wait: 400 });
    const panelReset = await panelOpen();
    await press(page, "j", { ctrl: true, wait: 300 });
    check("resetting it brings Ctrl+J back", (await panelOpen()) !== panelReset);
    await press(page, "j", { ctrl: true, wait: 300 });

    // Narrow: the six menus fold into one Menu button with the same model.
    await resize(page, 860, 700);
    await sleep(300);
    if (await exists(page, "#side-panel")) await press(page, "Escape", { wait: 300 });
    check(
      "at 860px the menus fold into one Menu button",
      (await exists(page, "header [data-menu-fold]")) &&
        !(await exists(page, "header [data-menu-bar]")),
    );
    await click(page, "header [data-menu-fold]");
    check(
      "the Menu button lists the same six menus",
      (await page.evaluate(
        `[...document.querySelectorAll('header [data-menu-row]')].map((b) => b.textContent.replace("›", "").trim()).join()`,
      )) === "File,Edit,View,Go,Build,Help",
    );
    await click(page, 'header [data-menu-row="2"]');
    check("choosing View opens its items beside the list", await menuOpen("View"));
    await shoot(page, "18h-menu-folded");
    const foldedBefore = await panelOpen();
    await click(page, 'header [role="menu"] [data-command="view.toggle-panel"]');
    check(
      "a folded menu item runs its command",
      (await panelOpen()) !== foldedBefore && !(await anyMenuOpen()),
    );
    await press(page, "j", { ctrl: true, wait: 300 });
    check(
      "at 860px the top bar fits without overflow",
      await page.evaluate(
        `(() => { const bar = document.querySelector("header"); return document.documentElement.scrollWidth <= window.innerWidth && bar.scrollWidth <= bar.clientWidth; })()`,
      ),
    );
    await resize(page, 1280, 820);
    await sleep(300);
    if (!(await exists(page, "#side-panel")))
      await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');
    await noStrayEdits(page, "menu bar, narrow");

    /* ------------------------------------------------------------------ */
    section("Versions");
    const dialogTitle = () =>
      page.evaluate(`document.querySelector("#dialog-title")?.textContent ?? null`);
    const dialogText = () =>
      page.evaluate(`document.querySelector('[role="alertdialog"]')?.innerText ?? ""`);
    const dialogOpen = () => exists(page, '[role="alertdialog"]');
    const versionRows = () =>
      page.evaluate(
        `[...document.querySelectorAll('#side-panel ul[aria-label="Versions"] > li')].map((li) => li.querySelector("button").innerText.replace(/\\s+/g, " ").trim())`,
      );
    const openMenuCommand = async (bar, command) => {
      await clickText(page, "header [data-menu-bar]", bar);
      await waitFor(
        page,
        `document.querySelector('header [role="menu"] [data-command="${command}"]') !== null`,
      );
      await click(page, `header [role="menu"] [data-command="${command}"]`);
    };
    /** Type at the top of the open document (the viewport stays on it). */
    const typeAtEnd = async (text) => {
      await focusEditor(page);
      await press(page, "Home", { ctrl: true });
      await insertText(page, text);
    };
    const pressDialogButton = async (label) => {
      const ok = await page.evaluate(`(() => {
        const b = [...document.querySelectorAll('[role="alertdialog"] button')].find((x) => x.textContent.trim() === ${JSON.stringify(label)});
        if (!b) return false;
        b.click();
        return true;
      })()`);
      if (!ok) throw new Error(`no dialog button "${label}"`);
      await sleep(300);
    };
    const pressPrimary = async () => {
      const ok = await page.evaluate(`(() => {
        const b = [...document.querySelectorAll('[role="alertdialog"] button')].find((x) => getComputedStyle(x).backgroundColor === "rgb(198, 241, 53)");
        if (!b) return false;
        b.click();
        return true;
      })()`);
      if (!ok) throw new Error("no lime dialog button");
      await sleep(300);
    };

    await focusEditor(page);
    await openMenuCommand("File", "file.save-version");
    await expectEventually(
      page,
      "File > Save a version opens a one-field dialog",
      `document.querySelector("#dialog-title")?.textContent === "Save a version"`,
    );
    check(
      "the dialog shows how many files changed and asks only for an optional name",
      (await dialogText()).includes("changed since the last version") ||
        (await dialogText()).includes("Nothing has changed"),
      await dialogText(),
    );
    await press(page, "Escape");

    // The first edit and its version.
    await typeAtEnd("\n// versions-check\n");
    await press(page, "s", { ctrl: true, alt: true, wait: 500 });
    await expectEventually(
      page,
      "Ctrl+Alt+S opens Save a version",
      `document.querySelector("#dialog-title")?.textContent === "Save a version"`,
    );
    check(
      "the field is labelled 'Name this version (optional)'",
      await page.evaluate(
        `document.querySelector('[role="alertdialog"] label')?.innerText.includes("Name this version (optional)")`,
      ),
    );
    check(
      "the dialog counts the changed files",
      /\d+ files? (has|have) changed since the last version/.test(await dialogText()),
      await dialogText(),
    );
    check(
      "Save is the one lime action in the dialog",
      await page.evaluate(`(() => {
        const lime = "rgb(198, 241, 53)";
        const bs = [...document.querySelectorAll('[role="alertdialog"] button')];
        const limed = bs.filter((b) => getComputedStyle(b).backgroundColor === lime);
        return limed.length === 1 && limed[0].textContent.trim() === "Save";
      })()`),
    );
    await shoot(page, "30-save-a-version");
    await insertText(page, "Draft one");
    await pressDialogButton("Save");
    await expectEventually(
      page,
      "saving shows a status message with the name",
      `document.querySelector('footer[aria-label="Status"]').innerText.includes('Saved version "Draft one"')`,
    );
    check("the dialog closes after saving", !(await dialogOpen()));
    check(
      "unsaved edits were saved first, so no tab is left unsaved",
      (await dirtyTabs(page)).length === 0,
    );

    // The second.
    await typeAtEnd("// second-edit\n");
    await openMenuCommand("File", "file.save-version");
    await waitFor(
      page,
      `document.querySelector('[role="alertdialog"] input') !== null`,
    );
    await insertText(page, "Draft two");
    await pressDialogButton("Save");
    await expectEventually(
      page,
      "a second version is saved from the menu",
      `document.querySelector('footer[aria-label="Status"]').innerText.includes('Saved version "Draft two"')`,
    );

    // The panel.
    await openMenuCommand("View", "view.versions");
    await expectEventually(
      page,
      "View > Versions opens the Source control view in Versions mode",
      `document.querySelector('aside[aria-label="Source control panel"]') !== null && document.querySelector('[role="radiogroup"][aria-label="Source control mode"] [role="radio"][aria-checked="true"]')?.textContent.trim() === "Versions"`,
    );
    check(
      "Versions adds no activity-bar entry",
      !(await exists(
        page,
        'nav[aria-label="Workbench views"] button[aria-label="Versions"]',
      )),
    );
    check(
      "the header switch offers Changes and Versions",
      (await page.evaluate(
        `[...document.querySelectorAll('[role="radiogroup"][aria-label="Source control mode"] [role="radio"]')].map((r) => r.textContent.trim()).join()`,
      )) === "Changes,Versions",
    );
    await expectEventually(
      page,
      "the timeline lists the saved versions newest first",
      `document.querySelectorAll('#side-panel ul[aria-label="Versions"] > li').length >= 4`,
    );
    {
      const rows = await versionRows();
      check(
        "the newest version is on top, marked as a Saved version",
        rows[0]?.includes("Draft two") === true && rows[0]?.includes("Saved version"),
        rows[0],
      );
      check(
        "each row shows author, relative time and file count",
        /· (just now|\d+m ago) · \d+ files?/.test(rows[0] ?? ""),
        rows[0],
      );
      check(
        "versions made outside Save a version carry no marker",
        rows.some((r) => !r.includes("Saved version")),
      );
    }
    await shoot(page, "31-versions-list");

    // Select, diff.
    await clickText(
      page,
      '#side-panel ul[aria-label="Versions"] > li > button',
      "Draft one",
    );
    await expectEventually(
      page,
      "selecting a version lists its changed files",
      `[...document.querySelectorAll('#side-panel ul[aria-label="Files in this version"] li')].some((li) => li.textContent.includes("main.typ"))`,
    );
    await expectEventually(
      page,
      "the file's diff shows the added line",
      `document.querySelector('#side-panel [role="region"][aria-label^="Changes in"]')?.innerText.includes("versions-check")`,
    );
    await shoot(page, "32-version-diff");

    // Restore a file (clean tree).
    await clickText(page, "#side-panel button", "Restore this file");
    await expectEventually(
      page,
      "Restore this file asks first",
      `document.querySelector("#dialog-title")?.textContent === "Restore this file?"`,
    );
    check(
      "the confirmation says plainly that nothing is deleted",
      (await dialogText()).includes("Nothing is deleted"),
      await dialogText(),
    );
    await pressDialogButton("Restore");
    await expectEventually(
      page,
      "restoring brings the old content back into the open tab",
      `(() => { const t = document.querySelector(".cm-content")?.innerText ?? ""; return t.includes("versions-check") && !t.includes("second-edit"); })()`,
      { show: `document.querySelector(".cm-content")?.innerText.slice(0, 120)` },
    );
    check(
      "a status message reports the restore",
      (await statusText(page)).includes("Restored main.typ"),
      await statusText(page),
    );
    check(
      "the restored tab is not marked unsaved",
      (await dirtyTabs(page)).length === 0,
      (await dirtyTabs(page)).join(),
    );

    // Dirty tree: the safety save is offered.
    await typeAtEnd("// unsaved-bit\n");
    await clickText(page, "#side-panel button", "Restore this file");
    await expectEventually(
      page,
      "restoring over unsaved work offers 'Save my changes first'",
      `[...document.querySelectorAll('[role="alertdialog"] button')].some((b) => b.textContent.includes("Save my changes first"))`,
    );
    check(
      "the confirmation names the safety version",
      (await dialogText()).includes("saved first") &&
        (await dialogText()).includes("Before restoring"),
      await dialogText(),
    );
    await shoot(page, "33-restore-dirty");
    await pressDialogButton("Save my changes first, then restore");
    await expectEventually(
      page,
      "the old content is back and the unsaved text was kept in a version",
      `(() => { const t = document.querySelector(".cm-content")?.innerText ?? ""; return !t.includes("unsaved-bit"); })()`,
    );
    await expectEventually(
      page,
      "a 'Before restoring' version is on top of the timeline",
      `document.querySelector('#side-panel ul[aria-label="Versions"] > li > button')?.innerText.includes("Before restoring")`,
    );

    // Restore the whole project from the oldest demo version.
    await clickText(
      page,
      '#side-panel ul[aria-label="Versions"] > li > button',
      "feat(bounds)",
    );
    await expectEventually(
      page,
      "an earlier version lists its files",
      `document.querySelectorAll('#side-panel ul[aria-label="Files in this version"] li').length >= 1`,
    );
    await clickText(page, "#side-panel button", "Restore whole project");
    await expectEventually(
      page,
      "Restore whole project asks first",
      `document.querySelector("#dialog-title")?.textContent === "Restore the whole project?"`,
    );
    await pressPrimary();
    await expectEventually(
      page,
      "restoring the whole project returns main.typ to that version",
      `(() => { const t = document.querySelector(".cm-content")?.innerText ?? ""; return t.includes("Landauer") && !t.includes("versions-check"); })()`,
    );

    // Filter by file.
    await clickText(
      page,
      '#side-panel [role="radiogroup"][aria-label="Show versions of"] [role="radio"]',
      "This file",
    );
    await expectEventually(
      page,
      "filtering to the current file keeps versions that changed it",
      `document.querySelectorAll('#side-panel ul[aria-label="Versions"] > li').length >= 1 && document.querySelector('#side-panel [role="radio"][aria-checked="true"]') !== null`,
    );
    await clickText(
      page,
      '#side-panel [role="radiogroup"][aria-label="Show versions of"] [role="radio"]',
      "All files",
    );

    // No identity yet: the prompt.
    await typeAtEnd("// identity-check\n");
    await page.evaluate(`localStorage.setItem("manang.mock.git-identity", "none")`);
    await press(page, "s", { ctrl: true, alt: true, wait: 500 });
    await expectEventually(
      page,
      "saving without an identity asks 'Your name and email' first",
      `document.querySelector("#dialog-title")?.textContent === "Your name and email"`,
    );
    check(
      "the prompt has name and email fields",
      (await page.evaluate(
        `document.querySelectorAll('[role="alertdialog"] input').length`,
      )) === 2,
    );
    check(
      "'Only for this project' is the default, 'Use for all projects' the alternative",
      await page.evaluate(
        `(() => { const r = [...document.querySelectorAll('[role="alertdialog"] [role="radio"]')]; return r.length === 2 && r[0].textContent.trim() === "Only for this project" && r[0].getAttribute("aria-checked") === "true" && r[1].textContent.trim() === "Use for all projects"; })()`,
      ),
    );
    await shoot(page, "34-identity");
    await insertText(page, "Ada Lovelace");
    await click(page, '[role="alertdialog"] label:nth-of-type(2) input');
    await insertText(page, "not an email");
    await pressDialogButton("Continue");
    check(
      "a bad email is refused with a reason and the prompt stays",
      (await dialogTitle()) === "Your name and email" &&
        (await dialogText()).includes("does not look like an email"),
      await dialogText(),
    );
    await page.evaluate(
      `(() => { const i = document.querySelectorAll('[role="alertdialog"] input')[1]; i.focus(); i.select(); })()`,
    );
    await insertText(page, "ada@example.org");
    await pressDialogButton("Continue");
    await expectEventually(
      page,
      "after the identity it continues to Save a version",
      `document.querySelector("#dialog-title")?.textContent === "Save a version"`,
    );
    await insertText(page, "By Ada");
    await pressDialogButton("Save");
    await expectEventually(
      page,
      "the version is saved under the new identity",
      `document.querySelector('#side-panel ul[aria-label="Versions"] > li > button')?.innerText.includes("Ada Lovelace")`,
    );
    await page.evaluate(`localStorage.removeItem("manang.mock.git-identity")`);

    check(
      "no version control is left unfinished: no dialog open",
      !(await dialogOpen()),
    );
    check(
      "the Versions panel obeys the brand: hairline switch, no radius, no shadow",
      await page.evaluate(
        `(() => { const g = getComputedStyle(document.querySelector('#side-panel [role="radiogroup"]')); const b = getComputedStyle(document.querySelector('#side-panel [role="radio"]')); return g.borderTopWidth === "1px" && g.borderTopLeftRadius === "0px" && b.boxShadow === "none"; })()`,
      ),
    );
    /* ------------------------------------------------------------------ */
    section("Share and get updates");
    const SHARE = 'section[aria-label="Share and get updates"]';
    const shareFlag = (value) =>
      page.evaluate(
        value === null
          ? `localStorage.removeItem("manang.mock.git-share")`
          : `localStorage.setItem("manang.mock.git-share", ${JSON.stringify(value)})`,
      );
    const shareText = () => page.evaluate(`document.querySelector('${SHARE}')?.innerText ?? ""`);
    const limeButtons = () =>
      page.evaluate(
        `[...document.querySelectorAll('${SHARE} button')].filter((b) => b.className.includes("bg-accent")).map((b) => b.textContent.trim())`,
      );
    check(
      "the Versions view has a Share group asking for an address on first use",
      (await exists(page, SHARE)) && (await exists(page, `${SHARE} input#share-address`)),
    );
    check(
      "before connecting there is no Share or Get updates button",
      !(await shareText()).includes("Get updates"),
      await shareText(),
    );
    await click(page, `${SHARE} input#share-address`);
    await insertText(page, "not an address");
    await clickText(page, `${SHARE} button`, "Connect");
    await expectEventually(
      page,
      "an address with spaces is refused with a reason, inline",
      `document.querySelector('${SHARE} [role="alert"]')?.innerText.includes("no spaces")`,
    );
    await page.evaluate(
      `(() => { const i = document.querySelector('${SHARE} input'); i.focus(); i.select(); })()`,
    );
    await insertText(page, "https://example.org/team/paper.git");
    await clickText(page, `${SHARE} button`, "Connect");
    await expectEventually(
      page,
      "a good address connects and shows a status line",
      `document.querySelector('${SHARE} [data-testid="share-status"]')?.innerText.includes("Not shared yet")`,
      { show: `document.querySelector('${SHARE}')?.innerText` },
    );
    check(
      "exactly one action is lime: Share",
      JSON.stringify(await limeButtons()) === '["Share"]',
      JSON.stringify(await limeButtons()),
    );
    await shoot(page, "35-share-first");
    await clickText(page, `${SHARE} button`, "Share");
    await expectEventually(
      page,
      "Share reports what it shared and the status becomes Up to date",
      `document.querySelector('${SHARE} [data-testid="share-status"]')?.innerText === "Up to date"`,
      { show: `document.querySelector('${SHARE}')?.innerText` },
    );
    check(
      "a status message says Shared",
      (await statusText(page)).includes("Shared "),
      await statusText(page),
    );
    check(
      "nothing is lime once everything is shared",
      (await limeButtons()).length === 0,
      JSON.stringify(await limeButtons()),
    );

    // Someone else shared: Share is refused in plain words, Get updates is suggested.
    await shareFlag("behind");
    await clickText(page, `${SHARE} button`, "Share");
    await expectEventually(
      page,
      "Share when others have shared first says to get updates first",
      `document.querySelector('footer[aria-label="Status"]').innerText.includes("get updates first")`,
    );
    await expectEventually(
      page,
      "the status line says Updates available and Get updates is the lime one",
      `document.querySelector('${SHARE} [data-testid="share-status"]')?.innerText.includes("Updates available")`,
      { show: `document.querySelector('${SHARE}')?.innerText` },
    );
    check(
      "exactly one action is lime: Get updates",
      JSON.stringify(await limeButtons()) === '["Get updates"]',
      JSON.stringify(await limeButtons()),
    );
    await shoot(page, "36-share-updates-available");
    await clickText(page, `${SHARE} button`, "Get updates");
    await expectEventually(
      page,
      "Get updates reports what arrived",
      `document.querySelector('footer[aria-label="Status"]').innerText.includes("Got 2 updates")`,
      { show: `document.querySelector('footer[aria-label="Status"]').innerText` },
    );
    check(
      "after getting updates the status no longer says Updates available",
      !(await shareText()).includes("Updates available"),
      await shareText(),
    );

    // Both changed the same file: stops, names it, changes nothing.
    await typeAtEnd("// share-local\n");
    await press(page, "s", { ctrl: true, alt: true, wait: 500 });
    await expectEventually(
      page,
      "a local version is saved before the clash",
      `document.querySelector("#dialog-title")?.textContent === "Save a version"`,
    );
    await insertText(page, "Local draft");
    await pressDialogButton("Save");
    await expectEventually(
      page,
      "the status line counts the version to share and Share is the lime one",
      `document.querySelector('${SHARE} [data-testid="share-status"]')?.innerText.includes("to share")`,
      { show: `document.querySelector('${SHARE}')?.innerText` },
    );
    check(
      "exactly one action is lime: Share (again)",
      JSON.stringify(await limeButtons()) === '["Share"]',
      JSON.stringify(await limeButtons()),
    );
    await shareFlag("conflicts");
    await clickText(page, `${SHARE} button`, "Get updates");
    await expectEventually(
      page,
      "a clash opens an explanation instead of an error",
      `document.querySelector("#dialog-title")?.textContent === "These changes cannot be combined automatically"`,
    );
    check(
      "it names the file and says nothing was changed",
      (await dialogText()).includes("main.") && (await dialogText()).includes("Nothing was changed"),
      await dialogText(),
    );
    await shoot(page, "37-share-conflict");
    await pressPrimary();

    // The host cannot be reached.
    await shareFlag("unreachable");
    await clickText(page, `${SHARE} button`, "Share");
    await expectEventually(
      page,
      "an unreachable host is explained in plain words",
      `document.querySelector('footer[aria-label="Status"]').innerText.includes("could not be reached")`,
      { show: `document.querySelector('footer[aria-label="Status"]').innerText` },
    );
    await shareFlag(null);
    // The refusals above were provoked on purpose and logged by reportError.
    for (let i = page.errors.length - 1; i >= 0; i -= 1) {
      if (/sharing versions|getting updates/.test(page.errors[i])) page.errors.splice(i, 1);
    }
    check(
      "the group uses no git jargon",
      !/\b(push|pull|fetch|merge|rebase|upstream|origin|commit)\b/i.test(await shareText()),
      await shareText(),
    );
    check(
      "the Share group obeys the brand: hairline buttons, no radius, no shadow",
      await page.evaluate(
        `(() => { const b = getComputedStyle(document.querySelector('${SHARE} button')); const g = getComputedStyle(document.querySelector('${SHARE}')); return b.borderTopLeftRadius === "0px" && b.boxShadow === "none" && g.borderBottomWidth === "1px"; })()`,
      ),
    );
    check("no dialog is left open", !(await dialogOpen()));

    await noStrayEdits(page, "Versions");
    await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');
    if (!(await exists(page, 'aside[aria-label="Files panel"]')))
      await click(page, 'nav[aria-label="Workbench views"] button[aria-label="Files"]');

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
    const crumb = (cdp) => cdp.evaluate(`document.title`);
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
    await waitFor(page, `document.title.includes("Lecture notes")`, {
      label: "switched to Lecture notes",
    });
    check(
      "switching opens the other project's main file",
      (await crumb(page)).includes("index.md — Lecture notes"),
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
    await waitFor(page, `document.title.includes("Entropy paper")`, {
      label: "switched back",
    });
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
    await waitFor(page, `document.title.includes("Lecture notes")`, {
      label: "switched after discarding",
    });
    check("discarding switches", (await crumb(page)).includes("Lecture notes"));
    await press(page, "o", { ctrl: true, synthetic: true });
    await insertText(page, "entropy");
    await press(page, "Enter", { wait: 500 });
    await waitFor(page, `document.title.includes("Entropy paper")`, {
      label: "back in Entropy paper",
    });

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
    const latexBox = await doc.evaluate(PAGE_BOX);
    check(
      "the toolbar says which page of how many",
      latexBox !== null && latexBox.page === 1 && latexBox.of >= 1,
      JSON.stringify(latexBox),
    );
    check(
      "the Compile button names the TeX backend, and the header stays quiet",
      await doc.evaluate(
        `document.querySelector('section[aria-label="Preview"] [aria-label="Refresh preview"]').title.includes("pdflatex, system TeX") &&
          !document.querySelector('section[aria-label="Preview"]').innerText.includes("system TeX")`,
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
      `document.querySelector('section[aria-label="Preview"] [aria-label="Refresh preview"]').title.includes("via main.tex")`,
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
    check(
      "the focused editor selection is the accent tint, not CodeMirror's lavender",
      await doc.evaluate(`(() => {
        const el = document.querySelector(".cm-selectionLayer .cm-selectionBackground");
        if (!el) return false;
        const m = getComputedStyle(el).backgroundColor.match(/[\\d.]+/g)?.map(Number);
        // The tint is lime over the base: green leads, blue never does.
        return !!m && m[1] > m[2] && m[0] > m[2] - 1;
      })()`),
    );
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
      `(${PAGE_BOX})?.of >= 2`,
      { timeout: 8000 },
    );
    await doc.evaluate(`document.querySelector("[data-pdf-viewer]").focus()`);
    await press(doc, "PageDown");
    await expectEventually(
      doc,
      "PageDown in the viewer scrolls to page 2",
      `(${PAGE_BOX})?.page === 2 && document.querySelector('[data-pdf-viewer]').scrollTop > 100`,
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

    // Contextual settings over a LaTeX document: LaTeX's, not Typst's.
    await press(doc, ",", { ctrl: true, wait: 400 });
    await expectEventually(
      doc,
      "from a LaTeX document, Settings show the LaTeX plugin and not Typst",
      `(() => { const g = [...document.querySelectorAll('section[aria-label="Settings"] [data-group]')].map((e) => e.dataset.group); return g.includes("plugin:latex") && g.includes("editor") && !g.includes("plugin:typst") && !g.includes("appearance") && (document.querySelector("[data-settings-scope]")?.innerText ?? "").includes("· LaTeX"); })()`,
      {
        show: `[...document.querySelectorAll('section[aria-label="Settings"] [data-group]')].map((e) => e.dataset.group).join()`,
      },
    );
    await shoot(doc, "23b-settings-latex");
    await press(doc, "Escape", { wait: 300 });

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
    section("Crash recovery");
    const rec = await newPage(browser);
    current = rec;
    await clickText(rec, "[data-project] button", "Entropy paper");
    await waitFor(
      rec,
      `document.querySelector(".cm-content")?.textContent.includes("Landauer")`,
      { label: "editor shows main.typ" },
    );
    const snapshotPaths = () =>
      rec.evaluate(
        `(() => { try { return Object.values(JSON.parse(localStorage.getItem("manang.recovery.v1") ?? "{}").snapshots ?? {}).map((s) => s.path); } catch { return []; } })()`,
      );
    const diskText = () =>
      rec.evaluate(
        `import("/src/lib/kernel/index.ts").then((m) => m.kernel.run("project.read-file", { path: "main.typ" }))`,
      );
    const recoveryDialog = `document.querySelector('[role="alertdialog"]')?.innerText ?? ""`;

    await focusEditor(rec);
    await caretToEnd(rec);
    await insertText(rec, "\nUNSAVED-RECOVERY-LINE\n");
    check(
      "typing makes the tab unsaved",
      (await dirtyTabs(rec)).includes("main.typ"),
      (await dirtyTabs(rec)).join(),
    );
    check(
      "no snapshot is written on every keystroke",
      !(await snapshotPaths()).includes("main.typ"),
    );
    let waited = 0;
    while (!(await snapshotPaths()).includes("main.typ") && waited < 8000) {
      await sleep(250);
      waited += 250;
    }
    check(
      "a recovery snapshot is written a few seconds after the last edit",
      (await snapshotPaths()).includes("main.typ") && waited >= 1000,
      `after ${waited} ms`,
    );
    check(
      "the file on disk is untouched by the snapshot",
      !(await diskText()).includes("UNSAVED-RECOVERY-LINE"),
    );

    // The crash: the page goes away with the edit unsaved.
    await reload(rec);
    await expectEventually(
      rec,
      "after the crash one quiet dialog offers the unsaved text back",
      `${recoveryDialog}.includes("Recover unsaved changes from")`,
      { timeout: 10000 },
    );
    check(
      "it names the file and offers Recover all, Review and Discard",
      await rec.evaluate(
        `(() => { const d = document.querySelector('[role="alertdialog"]'); const b = [...d.querySelectorAll("button")].map((x) => x.textContent.trim()); return d.innerText.includes("main.typ") && d.innerText.includes("in 1 file") && b.includes("Recover all") && b.includes("Review") && b.includes("Discard"); })()`,
      ),
    );
    check(
      "recovery has not touched the open document yet",
      !(await rec.evaluate(
        `document.querySelector(".cm-content")?.textContent.includes("UNSAVED-RECOVERY-LINE")`,
      )),
    );
    await shoot(rec, "30-recovery-offer");

    await clickText(rec, '[role="alertdialog"] button', "Review");
    await expectEventually(
      rec,
      "Review shows the recovered text against the disk",
      `(() => { const r = document.querySelector('[data-testid="recovery-review"]'); return !!r && r.innerText.includes("main.typ") && r.innerText.includes("UNSAVED-RECOVERY-LINE") && r.innerText.includes("File 1 of 1"); })()`,
    );
    await shoot(rec, "31-recovery-review");
    await clickText(rec, '[data-testid="recovery-review"] button', "Recover this");
    await expectEventually(
      rec,
      "Recover this opens the text as an unsaved edit",
      `document.querySelector(".cm-content")?.textContent.includes("UNSAVED-RECOVERY-LINE")`,
    );
    check(
      "the tab is marked unsaved and the review closed",
      (await dirtyTabs(rec)).includes("main.typ") &&
        !(await exists(rec, '[data-testid="recovery-review"]')),
    );
    check(
      "the file on disk still does not have the edit",
      !(await diskText()).includes("UNSAVED-RECOVERY-LINE"),
    );

    // Crash again; this time Recover all, then save.
    waited = 0;
    await focusEditor(rec);
    await caretToEnd(rec);
    await insertText(rec, "x");
    while (!(await snapshotPaths()).includes("main.typ") && waited < 8000) {
      await sleep(250);
      waited += 250;
    }
    await reload(rec);
    await expectEventually(
      rec,
      "a second crash offers it again",
      `${recoveryDialog}.includes("Recover unsaved changes from")`,
      { timeout: 10000 },
    );
    await clickText(rec, '[role="alertdialog"] button', "Recover all");
    await expectEventually(
      rec,
      "Recover all brings the text back as an unsaved edit",
      `document.querySelector(".cm-content")?.textContent.includes("UNSAVED-RECOVERY-LINE")`,
    );
    check("and the tab is unsaved", (await dirtyTabs(rec)).includes("main.typ"));
    await press(rec, "s", { ctrl: true });
    await expectEventually(
      rec,
      "saving writes the recovered text to the file",
      `import("/src/lib/kernel/index.ts").then((m) => m.kernel.run("project.read-file", { path: "main.typ" })).then((t) => t.includes("UNSAVED-RECOVERY-LINE"))`,
    );
    waited = 0;
    while ((await snapshotPaths()).includes("main.typ") && waited < 4000) {
      await sleep(100);
      waited += 100;
    }
    check("saving removes the snapshot", !(await snapshotPaths()).includes("main.typ"));
    await reload(rec);
    await sleep(1200);
    check(
      "a clean reload shows no recovery dialog",
      !(await rec.evaluate(`${recoveryDialog}.includes("Recover")`)),
    );

    // Discard.
    await focusEditor(rec);
    await caretToEnd(rec);
    await insertText(rec, "\nTO-BE-DISCARDED\n");
    waited = 0;
    while (!(await snapshotPaths()).includes("main.typ") && waited < 8000) {
      await sleep(250);
      waited += 250;
    }
    await reload(rec);
    await expectEventually(
      rec,
      "an unsaved edit is offered once more",
      `${recoveryDialog}.includes("Recover unsaved changes from")`,
      { timeout: 10000 },
    );
    await clickText(rec, '[role="alertdialog"] button', "Discard");
    await sleep(400);
    check(
      "Discard removes the snapshot and leaves the document alone",
      !(await snapshotPaths()).includes("main.typ") &&
        !(await rec.evaluate(
          `document.querySelector(".cm-content")?.textContent.includes("TO-BE-DISCARDED")`,
        )) &&
        (await dirtyTabs(rec)).length === 0,
    );
    // Closing a dirty tab with "Don't save" also discards the snapshot.
    await focusEditor(rec);
    await caretToEnd(rec);
    await insertText(rec, "\nGONE-ON-PURPOSE\n");
    waited = 0;
    while (!(await snapshotPaths()).includes("main.typ") && waited < 8000) {
      await sleep(250);
      waited += 250;
    }
    await click(rec, '[role="tab"][data-tab="main.typ"]', "middle");
    await expectEventually(
      rec,
      "closing an unsaved tab still asks",
      `${recoveryDialog}.includes("Save changes to main.typ")`,
    );
    check(
      "the snapshot is kept while the question is open",
      (await snapshotPaths()).includes("main.typ"),
    );
    await clickText(rec, '[role="alertdialog"] button', "Don't save");
    await sleep(400);
    check(
      "choosing Don't save discards the snapshot too",
      !(await snapshotPaths()).includes("main.typ"),
    );
    check(
      "no page errors during crash recovery",
      rec.errors.length === 0,
      rec.errors.slice(0, 2).join(" | "),
    );
    rec.close();
    current = page;

    /* ------------------------------------------------------------------ */
    section("Files that change on disk");
    const ext = await newPage(browser);
    current = ext;
    await clickText(ext, "[data-project] button", "Entropy paper");
    await waitFor(
      ext,
      `document.querySelector(".cm-content")?.textContent.includes("Landauer")`,
      { label: "editor shows main.typ" },
    );
    const outside = (command, args) =>
      ext.evaluate(
        `import("/src/lib/kernel/index.ts").then((m) => m.kernel.run(${JSON.stringify(command)}, ${JSON.stringify(args)}))`,
      );
    const diskOf = (path) =>
      ext.evaluate(
        `import("/src/lib/kernel/index.ts").then((m) => m.kernel.run("project.read-file", { path: ${JSON.stringify(path)} }))`,
      );
    const bar = `document.querySelector('[data-testid="external-change-bar"]')`;
    const editorHas = (text) =>
      `document.querySelector(".cm-content")?.textContent.includes(${JSON.stringify(text)})`;
    const mainText = await diskOf("main.typ");

    // A clean document follows the file silently, caret kept.
    await focusEditor(ext);
    await press(ext, "Home", { ctrl: true });
    await outside("dev.write-outside", {
      path: "main.typ",
      content: `${mainText}\nFROM-OUTSIDE-ONE\n`,
    });
    await expectEventually(
      ext,
      "a clean open document reloads silently when the file changes on disk",
      editorHas("FROM-OUTSIDE-ONE"),
    );
    check(
      "no bar, no unsaved mark, caret where it was",
      !(await ext.evaluate(`${bar} !== null`)) &&
        (await dirtyTabs(ext)).length === 0 &&
        /Ln 1, Col 1/.test(await statusText(ext)),
      await statusText(ext),
    );

    // A dirty document is never overwritten: the bar asks.
    await focusEditor(ext);
    await press(ext, "End", { ctrl: true });
    await insertText(ext, "\nMY-UNSAVED-EDIT\n");
    check("typing marks it unsaved", (await dirtyTabs(ext)).includes("main.typ"));
    await outside("dev.write-outside", {
      path: "main.typ",
      content: `${mainText}\nFROM-OUTSIDE-TWO\n`,
    });
    await expectEventually(
      ext,
      "a changed file under unsaved edits shows the bar above the editor",
      `${bar}?.dataset.status === "changed" && ${bar}.innerText.includes("main.typ changed on disk")`,
    );
    check(
      "the bar offers Keep my version, Load disk version and Compare",
      await ext.evaluate(
        `(() => { const b = [...${bar}.querySelectorAll("button")].map((x) => x.textContent.trim()); return b.join() === "Keep my version,Load disk version,Compare"; })()`,
      ),
    );
    check(
      "neither side was overwritten",
      (await ext.evaluate(editorHas("MY-UNSAVED-EDIT"))) &&
        !(await ext.evaluate(editorHas("FROM-OUTSIDE-TWO"))) &&
        (await diskOf("main.typ")).includes("FROM-OUTSIDE-TWO") &&
        (await dirtyTabs(ext)).includes("main.typ"),
    );
    await shoot(ext, "32-external-change-bar");
    await clickText(ext, '[data-testid="external-change-bar"] button', "Compare");
    await expectEventually(
      ext,
      "Compare shows the disk version against the unsaved text",
      `${bar}.innerText.includes("FROM-OUTSIDE-TWO") && ${bar}.innerText.includes("MY-UNSAVED-EDIT")`,
    );
    await shoot(ext, "33-external-compare");
    await clickText(ext, '[data-testid="external-change-bar"] button', "Compare");

    // Saving over a changed file asks first.
    await focusEditor(ext);
    await press(ext, "s", { ctrl: true });
    await expectEventually(
      ext,
      "saving a dirty document whose file changed asks before replacing it",
      `(() => { const d = document.querySelector('[role="alertdialog"]'); return !!d && d.innerText.includes("main.typ changed on disk") && [...d.querySelectorAll("button")].map((x) => x.textContent.trim()).join() === "Cancel,Overwrite with my version,Load disk version,Compare"; })()`,
    );
    await shoot(ext, "34-save-conflict");
    await press(ext, "Escape");
    await sleep(200);
    check(
      "cancelling writes nothing",
      (await diskOf("main.typ")).includes("FROM-OUTSIDE-TWO") &&
        !(await diskOf("main.typ")).includes("MY-UNSAVED-EDIT"),
    );
    await clickText(
      ext,
      '[data-testid="external-change-bar"] button',
      "Keep my version",
    );
    await sleep(200);
    check(
      "Keep my version clears the bar and stays unsaved",
      !(await ext.evaluate(`${bar} !== null`)) &&
        (await dirtyTabs(ext)).includes("main.typ"),
    );
    await focusEditor(ext);
    await press(ext, "s", { ctrl: true });
    await expectEventually(
      ext,
      "and the next save replaces the file on purpose",
      `import("/src/lib/kernel/index.ts").then((m) => m.kernel.run("project.read-file", { path: "main.typ" })).then((t) => t.includes("MY-UNSAVED-EDIT") && !t.includes("FROM-OUTSIDE-TWO"))`,
    );
    check("the tab is clean again", (await dirtyTabs(ext)).length === 0);

    // Load disk version drops the edits, and Ctrl+Z brings them back.
    await focusEditor(ext);
    await press(ext, "End", { ctrl: true });
    await insertText(ext, "\nSECOND-EDIT\n");
    await sleep(900); // CodeMirror groups edits closer than 500 ms into one undo step.
    const mineNow = await diskOf("main.typ");
    await outside("dev.write-outside", {
      path: "main.typ",
      content: `${mineNow}\nFROM-OUTSIDE-THREE\n`,
    });
    await expectEventually(
      ext,
      "a second outside change shows the bar again",
      `${bar}?.dataset.status === "changed"`,
    );
    await clickText(
      ext,
      '[data-testid="external-change-bar"] button',
      "Load disk version",
    );
    await expectEventually(
      ext,
      "Load disk version shows the file as it is on disk",
      `${editorHas("FROM-OUTSIDE-THREE")} && !(${editorHas("SECOND-EDIT")})`,
    );
    check(
      "the tab is clean and the bar is gone",
      (await dirtyTabs(ext)).length === 0 && !(await ext.evaluate(`${bar} !== null`)),
    );
    await focusEditor(ext);
    await press(ext, "z", { ctrl: true });
    await sleep(200);
    check(
      "undo brings the dropped edits back",
      await ext.evaluate(editorHas("SECOND-EDIT")),
    );
    await press(ext, "z", { ctrl: true });
    await press(ext, "z", { ctrl: true });

    // A file that goes away keeps its tab.
    await ext.evaluate(
      `document.querySelector('[role="treeitem"][data-path="notes.md"]').click()`,
    );
    await waitFor(
      ext,
      `document.querySelector('[role="tab"][data-tab="notes.md"]') !== null`,
    );
    await outside("dev.delete-outside", { path: "notes.md" });
    await expectEventually(
      ext,
      "a file deleted on disk keeps its tab and says so",
      `${bar}?.dataset.status === "deleted" && ${bar}.innerText.includes("notes.md was deleted or moved on disk") && document.querySelector('[role="tab"][data-tab="notes.md"]') !== null`,
    );
    check(
      "the bar offers Save again, Save as and Close",
      await ext.evaluate(
        `[...${bar}.querySelectorAll("button")].map((x) => x.textContent.trim()).join() === "Save again,Save as,Close"`,
      ),
    );
    check(
      "the text is still there",
      await ext.evaluate(editorHas("Check the dissipation curve")),
    );
    await shoot(ext, "35-external-deleted");
    await clickText(ext, '[data-testid="external-change-bar"] button', "Save again");
    await expectEventually(
      ext,
      "Save again writes the file back",
      `import("/src/lib/kernel/index.ts").then((m) => m.kernel.run("project.read-file", { path: "notes.md" })).then((t) => t.includes("dissipation curve"))`,
    );
    check("and the bar goes away", !(await ext.evaluate(`${bar} !== null`)));
    // Deleted again, then Close.
    await outside("dev.delete-outside", { path: "notes.md" });
    await expectEventually(
      ext,
      "deleted again",
      `${bar}?.dataset.status === "deleted"`,
    );
    await clickText(ext, '[data-testid="external-change-bar"] button', "Close");
    await sleep(300);
    check("Close closes the tab", !(await tabNames(ext)).includes("notes.md"));
    // A figure follows its scene file when it has no unsaved drawing.
    await ext.evaluate(
      `document.querySelector('[role="treeitem"][data-path="figures"]')?.click()`,
    );
    await waitFor(
      ext,
      `document.querySelector('[role="treeitem"][data-path="figures/dissipation_curve.figure.json"]') !== null`,
    );
    await ext.evaluate(
      `document.querySelector('[role="treeitem"][data-path="figures/dissipation_curve.figure.json"]').click()`,
    );
    await waitFor(ext, `document.querySelector("[data-node-id]") !== null`, {
      label: "figure shown",
    });
    const sceneText = await diskOf("figures/dissipation_curve.figure.json");
    const scene = JSON.parse(sceneText);
    scene.nodes.push({
      kind: "rect",
      id: "n99",
      x: 300,
      y: 200,
      width: 120,
      height: 60,
      stroke: "#e8e6e1",
      fill: "#1c1c20",
    });
    await outside("dev.write-outside", {
      path: "figures/dissipation_curve.figure.json",
      content: JSON.stringify(scene, null, 2),
    });
    await expectEventually(
      ext,
      "a clean open figure reloads when its scene file changes on disk",
      `document.querySelector('[data-node-id="n99"]') !== null`,
    );
    check(
      "and stays clean, with no bar",
      !(await ext.evaluate(`${bar} !== null`)) &&
        !(await dirtyTabs(ext)).includes("figures/dissipation_curve.figure.json"),
    );
    await ext.evaluate(
      `document.querySelector('[role="treeitem"][data-path="data"]')?.click()`,
    );
    await waitFor(
      ext,
      `document.querySelector('[role="treeitem"][data-path="data/measurements.csv"]') !== null`,
    );
    await ext.evaluate(
      `document.querySelector('[role="treeitem"][data-path="data/measurements.csv"]').click()`,
    );
    await waitFor(
      ext,
      `document.querySelector(".cm-content")?.textContent.includes("cycle")`,
    );
    await outside("dev.write-outside", {
      path: "data/measurements.csv",
      content: "cycle,duration_ms,heat_kT\n9,90,0.5\n",
    });
    await expectEventually(
      ext,
      "an open data file follows its file on disk",
      `document.querySelector(".cm-content")?.textContent.includes("9,90,0.5")`,
    );
    check(
      "no page errors while files changed on disk",
      ext.errors.length === 0,
      ext.errors.slice(0, 2).join(" | "),
    );
    ext.close();
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
  if (current !== undefined && current.errors.length > 0) {
    // What the page itself reported before the run died: usually the cause.
    process.stderr.write(
      `page reported: ${current.errors.slice(0, 5).join(" | ").slice(0, 1500)}\n`,
    );
  }
  process.exit(1);
});
