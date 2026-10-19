import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

/**
 * The lint rules of the interface.
 *
 * The rules of React are the ones that catch real mistakes: a hook
 * called conditionally, an effect without a cleanup, a dependency that
 * was left out. The rest is the recommended set of the language and of
 * the framework.
 */
export default tseslint.config(
  { ignores: ["dist", "node_modules", "coverage"] },

  js.configs.recommended,
  ...tseslint.configs.recommended,

  {
    files: ["**/*.{ts,tsx}"],
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],

      // The unused arguments are usually a sign of a half-finished
      // change, and the underscore says "this one is on purpose".
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
);
