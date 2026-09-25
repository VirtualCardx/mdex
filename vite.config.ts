import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Vite config for the mdex Tauri app.
// Port 1420 matches tauri.conf.json -> build.devUrl.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "chrome110",
  },
});
