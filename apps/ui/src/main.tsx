/** Entry point. No splash screen: the launcher starts on the screen it can use. */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { router } from "./router";
import "@minlauncher/visuals/tokens.css";
import "./styles/global.css";
import "./styles/shell.css";
import "./styles/components.css";
import "./styles/screens.css";

/** Short stale time: the backend is local, and versions disappear between clicks. */
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

const container = document.getElementById("root");
if (!container) {
  throw new Error("the root element is missing from the document");
}

createRoot(container).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
);
