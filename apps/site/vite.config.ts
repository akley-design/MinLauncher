import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  publicDir: "../ui/public",
  build: {
    target: "es2022",
    minify: "oxc",
    sourcemap: false,
  },
});
