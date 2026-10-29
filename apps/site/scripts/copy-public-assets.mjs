import { cp, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const root = dirname(fileURLToPath(import.meta.url));
const source = resolve(root, "../public");
const destination = resolve(root, "../dist");

await mkdir(destination, { recursive: true });
await cp(source, destination, { recursive: true, force: true });
