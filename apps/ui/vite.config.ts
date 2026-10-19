import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The dev server has to listen on a fixed port: the Tauri shell points at
// it by that address, and the port must not move when another project
// happens to use 1420.
export default defineConfig({
  plugins: [react()],

  // Tauri serves the frontend from a custom protocol, so the assets have
  // to be relative to the root of the document.
  base: "./",

  server: {
    port: 1420,
    strictPort: true,
    host: "localhost",
  },

  build: {
    // The launcher has to start fast, so the bundle is kept small and the
    // code is split per route.
    target: "es2022",
    // Vite 8 builds with rolldown, and the minifier that ships with it
    // is the one written in Rust. The esbuild one is not bundled and
    // would have to be installed on its own.
    minify: "oxc",
    sourcemap: false,
    chunkSizeWarningLimit: 700,
  },
});
