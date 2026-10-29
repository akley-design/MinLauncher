import React from "react";
import { createRoot } from "react-dom/client";
import { Site } from "./site";
import "@minlauncher/visuals/tokens.css";
import "./site.css";
import "./pages.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");

createRoot(root).render(
  <React.StrictMode>
    <Site />
  </React.StrictMode>,
);
