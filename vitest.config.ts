import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

/**
 * Unit tests for the shell's pure logic — AGENTS.md testing pyramid.
 *
 * Deliberately standalone rather than merged into `vite.config.ts`: these
 * suites exercise pure functions (scene graph, geometry, fuzzy search,
 * diagnostics) and need neither the Svelte compiler nor the Tailwind plugin.
 * Keeping them apart means the test runner has no reason to load the UI
 * toolchain, and a broken build config cannot silently green the test suite.
 *
 * No DOM environment here by design — the shell owns no engine state worth
 * exercising through a synthetic browser, and the visual layer is verified by
 * rendering it.
 */
export default defineConfig({
  resolve: {
    // Same alias as the app build; the `$lib` specifier must resolve identically
    // in tests or the imports under test would not be the ones that ship.
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    coverage: {
      provider: "v8",
      include: ["src/lib/**/*.ts"],
      exclude: ["src/lib/**/*.svelte.ts", "src/main.ts"],
    },
  },
});
