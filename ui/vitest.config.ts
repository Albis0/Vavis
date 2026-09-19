import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import react from "@vitejs/plugin-react";

// Separate from vite.config.ts so the build config stays about building.
// The two share nothing: the app is bundled for a WebView, the tests run
// in jsdom under node.
//
// Both plugins are present during the move to React, so the suites for the
// old interface and the new one run in one command -- a rewrite that only
// its own tests pass is not evidence of anything.
export default defineConfig({
  plugins: [svelte({ hot: false }), react()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    setupFiles: ["src/react/test-setup.ts"],
    // Svelte 5 runes only work in files the compiler processes, which for
    // plain `.ts` means opting in explicitly.
    server: { deps: { inline: ["svelte"] } },
  },
  resolve: {
    // Vitest would otherwise pick Svelte's server build, where the runes
    // that back the store are inert.
    conditions: ["browser"],
  },
});
