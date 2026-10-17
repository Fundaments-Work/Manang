import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  // `svelte()` returns an array of plugins, so the list is flattened to keep
  // Vite 8's stricter PluginOption typing satisfied.
  plugins: [tailwindcss(), svelte()].flat(),

  resolve: {
    // Plain Svelte has no built-in `$lib`, so Vite needs the alias declared
    // here as well as in tsconfig `paths`. SvelteKit provides this for free;
    // without it the dev server fails to resolve `$lib/...` imports at runtime
    // while `svelte-check` still passes, so the two must stay in sync.
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },

  // Vite options tailored for Tauri development and only applied in
  // `tauri dev` or `tauri build`.
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },

  // Tauri ships a modern webview on every supported platform, so the build
  // target can stay modern and free of transpilation overhead.
  build: {
    target: "es2022",
    // Match the webview's origin: no code splitting, no module preload.
    // AGENTS.md forbids base64/JSON payloads across the IPC boundary, and
    // this keeps the startup parse graph a single deterministic chunk.
    modulePreload: { polyfill: false },
    sourcemap: false,
  },
}));
