import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ isSsrBuild }) => ({
  plugins: [react()],
  publicDir: isSsrBuild ? false : "../ui/public",
  ssr: {
    noExternal: ["@minlauncher/visuals"],
  },
  build: {
    target: "es2022",
    minify: "oxc",
    sourcemap: false,
    copyPublicDir: !isSsrBuild,
    modulePreload: { polyfill: false },
    ...(isSsrBuild
      ? {}
      : {
          rolldownOptions: {
            output: {
              manualChunks(id: string) {
                if (id.includes("/node_modules/three/")) return "three";
                if (
                  id.includes("/node_modules/react/") ||
                  id.includes("/node_modules/react-dom/") ||
                  id.includes("/node_modules/scheduler/")
                )
                  return "react";
              },
            },
          },
        }),
  },
}));
