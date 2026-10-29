import { StrictMode } from "react";
import { hydrateRoot, createRoot } from "react-dom/client";
import { Site } from "./site";

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");

const pathname = window.location.pathname;
const tree = (
  <StrictMode>
    <Site pathname={pathname} />
  </StrictMode>
);

if (document.documentElement.dataset.prerendered === "public") {
  hydrateRoot(root, tree);
} else {
  createRoot(root).render(tree);
}
