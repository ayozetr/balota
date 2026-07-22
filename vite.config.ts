import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  // Tauri prints its own build output; do not wipe it.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Rust sources are watched by Tauri itself. Watching them here too makes
    // every backend rebuild trigger a pointless frontend reload.
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    // WebKitGTK 2.38+ handles everything in this target.
    target: "es2021",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
