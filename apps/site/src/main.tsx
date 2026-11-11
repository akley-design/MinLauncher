import React from "react";
import { createRoot } from "react-dom/client";
import { ActivityProgressProvider } from "@minlauncher/activity-progress";
import { Site } from "./site";
import { supabase } from "./supabase";
import "@minlauncher/visuals/tokens.css";
import "./site.css";
import "./pages.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");

createRoot(root).render(
  <React.StrictMode>
    <ActivityProgressProvider client={supabase} source="site">
      <Site />
    </ActivityProgressProvider>
  </React.StrictMode>,
);
