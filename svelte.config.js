import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/** @type {import('@sveltejs/vite-plugin-svelte').SvelteConfig} */
const config = {
  preprocess: vitePreprocess(),
  compilerOptions: {
    // Manang is a desktop app on a modern webview: runes mode everywhere,
    // no legacy reactive statements, no accessor-keyed props.
    runes: true,
  },
};

export default config;
