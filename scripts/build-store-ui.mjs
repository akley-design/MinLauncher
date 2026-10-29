import { spawnSync } from "node:child_process";

const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const result = spawnSync(
  npm,
  ["run", "build", "--workspace", "@minlaucher/ui"],
  {
    env: { ...process.env, VITE_STORE_BUILD: "true" },
    stdio: "inherit",
    shell: process.platform === "win32",
  },
);

if (result.error) throw result.error;
process.exit(result.status ?? 1);
