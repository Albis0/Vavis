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
  },
});
