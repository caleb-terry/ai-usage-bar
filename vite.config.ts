import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @tauri-apps/cli sets TAURI_DEV_HOST when developing on a physical device.
const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Vite options tailored for Tauri development.
  // Prevent Vite from obscuring Rust errors.
  clearScreen: false,
  server: {
    // Honor an injected PORT (Claude Preview assigns a free port when 1420 is
    // taken); fall back to the Tauri-conventional 1420 for normal dev.
    port: process.env.PORT ? Number(process.env.PORT) : 1420,
    // Tauri expects the dev server on a fixed port, so fail loudly if it's taken
    // — unless a PORT was injected (Claude Preview), where any free port is fine.
    strictPort: !process.env.PORT,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Don't watch the Rust backend.
      ignored: ["**/src-tauri/**"],
    },
  },
}));
