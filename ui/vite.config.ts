import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port; a random one would leave the dev server
// unreachable.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    // Tauri ships its own WebView, so support for old browsers is dead weight.
    target: "chrome110",
    minify: "esbuild",
    sourcemap: false,
    // Three.js is most of the bundle and is deliberately not split out: the
    // whole app is one local file served from inside the exe, so there is no
    // network to save a round trip on, and the reactor is on screen from the
    // first frame -- lazy-loading it would only add a blank centre.
    chunkSizeWarningLimit: 1200,
  },
});
