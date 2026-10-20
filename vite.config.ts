import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "./src") },
  },
  // Tauri expects a fixed port and must not have the dev server clear its output.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Rust build output changes constantly; Tauri watches src-tauri itself.
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
