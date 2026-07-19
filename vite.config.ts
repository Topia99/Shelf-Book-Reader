import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev server port
const tauriHost = process.env.TAURI_DEV_HOST;

export default defineConfig(({ command }) => ({
  plugins: [react()],
  clearScreen: false,
  // Standalone Tauri bundles are loaded from the app container rather than a web root.
  // Relative production asset paths avoid iOS/mobile white screens after reinstall/export.
  base: command === "build" ? "./" : "/",
  server: {
    host: tauriHost || undefined,
    hmr: tauriHost
      ? {
          protocol: "ws",
          host: tauriHost,
          port: 1421,
        }
      : undefined,
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
