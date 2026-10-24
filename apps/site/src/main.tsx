import React from "react";
import { createRoot } from "react-dom/client";
import { Site } from "./site";
import "./site.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");

createRoot(root).render(
  <React.StrictMode>
    <Site />
  </React.StrictMode>,
);
