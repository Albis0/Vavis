import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Separate from vite.config.ts so the build config stays about building.
// The two share nothing: the app is bundled for a WebView, the tests run
// in jsdom under node.
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    setupFiles: ["src/react/test-setup.ts"],
    // One jsdom per worker instead of one per file: building it was 60% of
    // the run. vmThreads still gives every file its own module graph, so
    // `vi.mock` in one file cannot leak into the next.
    pool: "vmThreads",
  },
});
