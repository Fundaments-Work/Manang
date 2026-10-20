// Runs the Rust relay client's conformance test (crates/manang-relay/tests/
// conformance.rs, and the paced bulk-sync replay in bulk_sync.rs) against this Worker under a local `wrangler dev`.
//
//   pnpm conformance            (needs a Rust toolchain; builds only manang-relay)
//
// Starts wrangler on 127.0.0.1:8787 (never 1420), waits for /health, runs the
// test, and stops wrangler again by the PID it started.

import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const worker = resolve(here, "..");
const repo = resolve(worker, "..", "..");
const port = process.env.RELAY_PORT ?? "8787";

const wrangler = spawn(
  "pnpm",
  ["exec", "wrangler", "dev", "--local", "--ip", "127.0.0.1", "--port", port],
  { cwd: worker, stdio: ["ignore", "pipe", "pipe"], detached: true },
);
wrangler.stdout.resume();
wrangler.stderr.resume();

const stop = () => {
  try {
    process.kill(-wrangler.pid, "SIGTERM");
  } catch {
    // already gone
  }
};
process.on("SIGINT", () => {
  stop();
  process.exit(130);
});

async function healthy() {
  for (let i = 0; i < 120; i++) {
    try {
      const res = await fetch(`http://127.0.0.1:${port}/health`);
      if (res.ok && (await res.text()).trim() === "ok") return true;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  return false;
}

let code = 1;
try {
  if (!(await healthy()))
    throw new Error("wrangler dev did not become healthy in 60 s");
  code = await new Promise((done) => {
    const cargo = spawn(
      "cargo",
      [
        "test",
        "-p",
        "manang-relay",
        "--test",
        "conformance",
        "--test",
        "bulk_sync",
        "--",
        "--nocapture",
      ],
      {
        cwd: repo,
        stdio: "inherit",
        env: {
          ...process.env,
          MANANG_RELAY_URL: `ws://127.0.0.1:${port}`,
          CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR ?? resolve(repo, "target"),
          CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "3",
        },
      },
    );
    cargo.on("exit", (c) => done(c ?? 1));
  });
} catch (e) {
  console.error(String(e));
} finally {
  stop();
}
process.exit(code);
