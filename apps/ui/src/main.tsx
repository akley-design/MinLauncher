/**
 * The entry point of the interface.
 *
 * The order of the three things that happen here matters:
 *
 * 1. the query client is created, so that the screens can ask the
 *    backend for what they need;
 * 2. the router is created, so that the screens have somewhere to be;
 * 3. the tree is mounted.
 *
 * Nothing else is done at the start. A launcher that shows a splash
 * screen while it loads something is a launcher that has already lost
 * the race it was built for.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { router } from "./router";
import "./styles/global.css";
import "./styles/redesign.css";

/**
 * The client of the queries.
 *
 * The launcher talks to a local backend, so the network is fast and the
 * data is never stale for long. A long stale time means the player
 * clicks "Играть" and the launcher sends the player to a version that
 * was uninstalled an hour ago.
 */
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
