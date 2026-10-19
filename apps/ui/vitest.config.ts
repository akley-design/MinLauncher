import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

/**
 * The test setup of the interface.
 *
 * The environment is jsdom, because the tests render components. The
 * Tauri bridge is mocked in the tests that need it: the tests must not
 * depend on a running desktop application.
 */
export default defineConfig({
  plugins: [react()],

  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    restoreMocks: true,
  },
});
