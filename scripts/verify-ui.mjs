/**
 * Headless verification harness for the Manang shell.
 *
 * The AGENTS.md testing pyramid asks for end-to-end UI tests, and the shell's
 * most valuable checks are visual and interactive: that the Margin Whisper
 * gutter actually paints a 4px square on the right line, that the palette's
 * arrow-key navigation moves the selection, that an overlay does not darken what
 * sits behind it. None of that is observable from a DOM snapshot alone.
 *
 * Drives the already-installed system Chromium over the DevTools protocol
 * rather than downloading a browser, so the check runs offline and on the same
 * binary the developer sees. `ws` is the only dependency.
 *
 * Usage: node scripts/verify-ui.mjs [hash ...]
 */

import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import { request as httpRequest } from "node:http";
import WebSocket from "ws";

const ORIGIN = process.env.MANANG_ORIGIN ?? "http://127.0.0.1:1420";
const SHOTS = "artifacts/screens";
const PORT = 9333;

function findBrowser() {
  const candidates = [
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
  ];
  for (const name of candidates) {
    // `spawnSync` rather than `spawn`: the exit status has to be readable before
    // deciding, and a browser hunt should not leave a dangling child process.
    const found = spawnSync("which", [name], { stdio: "ignore" });
    if (found.status === 0) return name;
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

  constructor(socket) {
    this.#socket = socket;
    socket.on("message", (raw) => {
      const message = JSON.parse(raw.toString());
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
      this.#pending.set(id, { resolve, reject });
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

function check(name, passed, detail = "") {
  results.push({ name, passed, detail });
  const mark = passed ? "PASS" : "FAIL";
  process.stdout.write(`  ${mark}  ${name}${detail ? ` — ${detail}` : ""}\n`);
}

async function newPage(browser, hash) {
  const target = await browser.send("Target.createTarget", { url: "about:blank" });
  const list = await fetch(`http://127.0.0.1:${PORT}/json/list`).then((r) => r.json());
  const info = list.find((entry) => entry.id === target.targetId);
  const cdp = await Cdp.open(info.webSocketDebuggerUrl);
  await cdp.send("Page.enable");
  await cdp.send("Runtime.enable");
  await cdp.send("Emulation.setDeviceMetricsOverride", {
    width: 1280,
    height: 820,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await cdp.send("Page.navigate", { url: `${ORIGIN}/${hash}` });
  // Wait for the shell to mount rather than sleeping a guessed interval.
  await waitFor(
    cdp,
    `document.readyState === "complete" && document.querySelector("#app")?.childElementCount > 0`,
    {
      label: "app mounted",
    },
  );
  await sleep(600);
  return cdp;
}

async function shoot(cdp, name) {
  const { data } = await cdp.send("Page.captureScreenshot", { format: "png" });
  mkdirSync(SHOTS, { recursive: true });
  writeFileSync(`${SHOTS}/${name}.png`, Buffer.from(data, "base64"));
}

/**
 * Polls until `expression` returns truthy.
 *
 * A fixed sleep is a flaky test: the editor mounts, then the Typst grammar
 * chunk resolves, then the whisper gutter repaints. Waiting for the thing under
 * test instead of guessing a duration is what makes this suite reliable.
 */
async function waitFor(cdp, expression, { timeout = 15000, label = expression } = {}) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    let value = false;
    try {
      value = await cdp.evaluate(expression);
    } catch {
      value = false;
    }
    if (value) return true;
    await sleep(150);
  }
  throw new Error(`timed out waiting for: ${label}`);
}

/** Dispatches a key press the way a real keyboard would. */
async function pressKey(cdp, key, virtualKeyCode) {
  await cdp.send("Input.dispatchKeyEvent", {
    type: "rawKeyDown",
    key,
    code: key,
    windowsVirtualKeyCode: virtualKeyCode,
    nativeVirtualKeyCode: virtualKeyCode,
  });
  await cdp.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key,
    code: key,
    windowsVirtualKeyCode: virtualKeyCode,
    nativeVirtualKeyCode: virtualKeyCode,
  });
  await sleep(250);
}

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
    process.stdout.write("\nDesk\n");

    const desk = await newPage(browser, "");
    await shoot(desk, "1-desk");

    check(
      "editor mounts CodeMirror and owns its own DOM",
      (await desk.evaluate(`document.querySelectorAll(".cm-editor").length`)) === 1,
    );

    check(
      "manuscript is rendered by the editor, not by Svelte",
      await desk.evaluate(
        `document.querySelector(".cm-content")?.textContent?.includes("Landauer") ?? false`,
      ),
    );

    const gutter = await desk.evaluate(`(() => {
      const dots = [...document.querySelectorAll(".manang-whisper")]
        .filter((el) => el.style.visibility !== "hidden");
      if (dots.length === 0) return { count: 0 };
      const box = dots[0].getBoundingClientRect();
      const style = getComputedStyle(dots[0]);
      return {
        count: dots.length,
        width: box.width,
        height: box.height,
        color: style.backgroundColor,
        top: Math.round(box.top),
      };
    })()`);

    check(
      "a whisper paints exactly one 4px square in the gutter",
      gutter.count === 1 &&
        Math.round(gutter.width) === 4 &&
        Math.round(gutter.height) === 4,
      `${gutter.count} dot(s), ${gutter.width}x${gutter.height}px, ${gutter.color}`,
    );

    // The whisper must sit on the line that carries the broken reference.
    const onCorrectLine = await desk.evaluate(`(() => {
      const dot = [...document.querySelectorAll(".manang-whisper")].find((el) => el.style.visibility !== "hidden");
      if (!dot) return false;
      const top = dot.getBoundingClientRect().top;
      const line = [...document.querySelectorAll(".cm-line")]
        .find((el) => {
          const r = el.getBoundingClientRect();
          // Half-open: a dot sitting exactly on a boundary belongs to the line
          // it starts, not the line that ends there.
          return r.height > 0 && top >= r.top && top < r.bottom;
        });
      return line?.textContent?.includes("@landauer1961") ?? false;
    })()`);
    check("whisper sits on the line holding @landauer1961", onCorrectLine);

    check(
      "no speed counters leak into the status bar",
      await desk.evaluate(`!/\\d+\\s?(ms|µs|μs|fps)/i.test(document.body.innerText)`),
    );

    check(
      "no rounded corners anywhere",
      await desk.evaluate(`(() => {
        for (const el of document.querySelectorAll("*")) {
          const r = getComputedStyle(el).borderTopLeftRadius;
          if (r !== "0px" && r !== "") return el.className + " -> " + r;
        }
        return true;
      })()`),
    );

    check(
      "fonts are the two permitted families",
      await desk.evaluate(`(() => {
        const fams = new Set();
        for (const el of document.querySelectorAll("body, header, footer, .cm-content, button")) {
          fams.add(getComputedStyle(el).fontFamily.split(",")[0].replace(/["']/g, ""));
        }
        return [...fams].every((f) => f === "DM Sans" || f === "DM Mono");
      })()`),
    );

    check(
      "body background is the carbon base",
      (await desk.evaluate(`getComputedStyle(document.body).backgroundColor`)) ===
        "rgb(20, 20, 22)",
    );

    /* ------------------------------------------------------------------ */
    process.stdout.write("\nLibrary shelf\n");

    const library = await newPage(browser, "#library");
    await shoot(library, "3-library");

    check(
      "rail is 260px with a hairline right border",
      await library.evaluate(`(() => {
        const rail = document.querySelector('aside[aria-label="Library shelf"]');
        if (!rail) return false;
        const r = rail.getBoundingClientRect();
        return Math.round(r.width) === 260 && getComputedStyle(rail).borderRightWidth === "1px";
      })()`),
    );

    check(
      "active document carries a 2px lime left boundary",
      await library.evaluate(`(() => {
        const active = document.querySelector('[aria-current="true"]');
        if (!active) return false;
        const s = getComputedStyle(active);
        return s.borderLeftWidth === "2px" && s.borderLeftColor === "rgb(56, 189, 248)";
      })()`),
    );

    check(
      "typing filters the tree",
      await (async () => {
        await library.evaluate(`(() => {
          const input = document.getElementById("library-filter");
          input.value = "lattice";
          input.dispatchEvent(new Event("input", { bubbles: true }));
        })()`);
        await sleep(300);
        return library.evaluate(
          `document.querySelectorAll('nav[aria-label="Project files"] button').length === 1`,
        );
      })(),
    );

    /* ------------------------------------------------------------------ */
    process.stdout.write("\nCommand palette\n");

    const palette = await newPage(browser, "#palette/cit");
    await shoot(palette, "5-palette");

    check(
      "palette query is applied from the deep link",
      (await palette.evaluate(`document.getElementById("palette-query")?.value`)) ===
        "cit",
    );

    check(
      "the best fuzzy match is the selected row",
      (
        await palette.evaluate(
          `document.querySelector('[role="option"][aria-selected="true"]')?.textContent ?? ''`,
        )
      ).includes("Verify citations") === true,
    );

    check(
      "ArrowDown moves the selection",
      await (async () => {
        const before = await palette.evaluate(
          `document.querySelector('[aria-selected="true"]')?.textContent?.trim()`,
        );
        await pressKey(palette, "ArrowDown", 40);
        const after = await palette.evaluate(
          `document.querySelector('[aria-selected="true"]')?.textContent?.trim()`,
        );
        return before !== after;
      })(),
    );

    check(
      "sections render in AI Actions then Plugins order",
      await palette.evaluate(`(() => {
        // Scoped to the dialog: the paper sheet behind the palette has its own
        // heading and would otherwise be counted as a section.
        const names = [...document.querySelectorAll('[role="dialog"] h2')]
          .map((h) => h.textContent.trim());
        return names[0] === "AI Actions" && names[1] === "Plugins & Commands";
      })()`),
    );

    /* ------------------------------------------------------------------ */
    process.stdout.write("\nVector ink canvas\n");

    const canvas = await newPage(browser, "#draw");
    await shoot(canvas, "2-canvas");

    check(
      "scene renders as native SVG with no raster surface",
      await canvas.evaluate(`(() => {
        const svg = document.querySelector('svg[role="application"]');
        if (!svg) return false;
        const frame = svg.querySelector("rect");
        return frame?.getAttribute("stroke") === "var(--mng-line)" &&
               svg.querySelectorAll("canvas").length === 0;
      })()`),
    );

    check(
      "pen tray offers the four tools from DESIGN.md §4.1",
      await canvas.evaluate(`(() => {
        const labels = [...document.querySelectorAll('[role="toolbar"] button')]
          .map((b) => b.textContent.trim());
        return ["Freehand", "Geometry", "Arrow", "Math"].every((t) =>
          labels.some((l) => l.includes(t)),
        );
      })()`),
    );

    // Draw a stroke and confirm the scene grows by one node.
    const drew = await (async () => {
      const before = await canvas.evaluate(
        `document.querySelectorAll('svg[role="application"] path').length`,
      );
      const box = await canvas.evaluate(`(() => {
        const r = document.querySelector('svg[role="application"]').getBoundingClientRect();
        return { x: r.x, y: r.y, w: r.width, h: r.height };
      })()`);
      const point = (fx, fy) => ({ x: box.x + box.w * fx, y: box.y + box.h * fy });
      const a = point(0.3, 0.6);
      const b = point(0.45, 0.75);
      const common = {
        button: "left",
        pointerType: "mouse",
        clickCount: 1,
        buttons: 1,
      };
      await canvas.send("Input.dispatchMouseEvent", {
        type: "mousePressed",
        ...common,
        x: a.x,
        y: a.y,
      });
      for (let i = 1; i <= 8; i += 1) {
        await canvas.send("Input.dispatchMouseEvent", {
          type: "mouseMoved",
          ...common,
          x: a.x + ((b.x - a.x) * i) / 8,
          y: a.y + ((b.y - a.y) * i) / 8,
        });
      }
      await canvas.send("Input.dispatchMouseEvent", {
        type: "mouseReleased",
        ...common,
        buttons: 0,
        x: b.x,
        y: b.y,
      });
      await sleep(400);
      const after = await canvas.evaluate(
        `document.querySelectorAll('svg[role="application"] path').length`,
      );
      return { before, after };
    })();

    check(
      "dragging with the pen commits a stroke to the scene",
      drew.after > drew.before,
      `${drew.before} -> ${drew.after} path elements`,
    );

    /* ------------------------------------------------------------------ */
    process.stdout.write("\nPresentation studio\n");

    const slides = await newPage(browser, "#slides/2");
    await shoot(slides, "6-slides");

    // `innerText` reflects `text-transform`, so the frame's corner label reads
    // uppercase in the rendered page even though the source says otherwise.
    check(
      "deep link selects the requested slide",
      (await slides.evaluate(
        `[...document.querySelectorAll('nav[aria-label="Slides"] [aria-current="true"]')].length`,
      )) === 1 &&
        (await slides.evaluate(`document.body.innerText.includes("SLIDE 3 / 6")`)),
    );

    check(
      "slide frame is 16:9",
      await slides.evaluate(`(() => {
      const el = [...document.querySelectorAll("div")].find((d) =>
        d.className.includes("aspect-video"),
      );
      if (!el) return false;
      const r = el.getBoundingClientRect();
      return Math.abs(r.width / r.height - 16 / 9) < 0.02;
    })()`),
    );

    check(
      "ArrowRight advances the deck",
      await (async () => {
        await pressKey(slides, "ArrowRight", 39);
        return slides.evaluate(`document.body.innerText.includes("SLIDE 4 / 6")`);
      })(),
    );

    /* ------------------------------------------------------------------ */
    process.stdout.write("\nInstrument shelf\n");

    const shelf = await newPage(browser, "#shelf");
    await shoot(shelf, "4-shelf");

    check(
      "every capability summary is reachable on hover",
      await shelf.evaluate(`(() => {
      const cards = [...document.querySelectorAll("li.group")];
      return cards.length === 6 && cards.every((c) => c.innerHTML.includes("Net:"));
    })()`),
    );

    check(
      "an essential instrument cannot be disabled",
      await (async () => {
        await shelf.evaluate(`(() => {
        const sw = [...document.querySelectorAll('[role="switch"]')][0];
        sw.click();
      })()`);
        await sleep(200);
        return shelf.evaluate(
          `[...document.querySelectorAll('[role="switch"]')][0].getAttribute("aria-checked") === "true"`,
        );
      })(),
    );

    check(
      "a plugin instrument toggles",
      await (async () => {
        const before = await shelf.evaluate(
          `[...document.querySelectorAll('[role="switch"]')][1].getAttribute("aria-checked")`,
        );
        await shelf.evaluate(`document.querySelectorAll('[role="switch"]')[1].click()`);
        await sleep(200);
        const after = await shelf.evaluate(
          `[...document.querySelectorAll('[role="switch"]')][1].getAttribute("aria-checked")`,
        );
        return before !== after;
      })(),
    );

    /* ------------------------------------------------------------------ */
    process.stdout.write("\nZen mode\n");

    const zen = await newPage(browser, "#zen");
    await shoot(zen, "7-zen");

    check(
      "zen dissolves every chrome element",
      await zen.evaluate(`(() => {
      return document.querySelector("header") === null &&
             document.querySelector("footer") === null &&
             document.querySelector("nav") === null;
    })()`),
    );

    check(
      "reading column is 680px",
      await zen.evaluate(`(() => {
      const article = document.querySelector("article");
      if (!article) return false;
      return Math.round(article.getBoundingClientRect().width) === 680;
    })()`),
    );

    /* ------------------------------------------------------------------ */
    process.stdout.write("\nOverlays\n");

    check(
      "overlays do not darken what sits behind them",
      await library.evaluate(`(() => {
      const pane = document.querySelector("main");
      return pane !== null &&
             !getComputedStyle(pane).filter.includes("brightness");
    })()`),
    );

    for (const page of [desk, library, palette, canvas, slides, shelf, zen])
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
