import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port; a random one would leave the dev server
// unreachable.
//
// Both plugins are loaded during the move to React. They do not conflict:
// each claims files by extension, and the React plugin is scoped to the
// directory holding the rewritten components so it never tries to transform
// a `.ts` file that Svelte's store still owns. When the last `.svelte` file
// is gone, the svelte plugin and its scope come out together.
export default defineConfig({
  plugins: [
    svelte(),
    react({ include: ["src/react/**/*.{ts,tsx}", "src/main.tsx"] }),
  ],
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
