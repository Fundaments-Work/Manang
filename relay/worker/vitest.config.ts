import { cloudflareTest } from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";

// Unit tests run inside workerd (the real Workers runtime, via miniflare) with
// the Worker's own Durable Object bindings. Limits are lowered through `vars`
// so the limit tests stay small and fast.
export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./wrangler.toml" },
      miniflare: {
        bindings: {
          MAX_FRAME_BYTES: "4096",
          MAX_PEERS_PER_ROOM: "3",
          RATE_MSGS_PER_SEC: "20",
          RATE_PRESENCE_PER_SEC: "2",
          IDLE_TIMEOUT_SECS: "2",
          RATE_CONNECTS_PER_MIN: "0",
          RATE_ROOMS_PER_10MIN: "0",
        },
      },
    }),
  ],
  test: {
    include: ["test/**/*.test.ts"],
    exclude: ["test/conformance/**"],
  },
});
