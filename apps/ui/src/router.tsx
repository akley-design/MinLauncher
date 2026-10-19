/**
 * The routes of the launcher.
 *
 * The router is built in code, not from files. A launcher has five
 * screens and they never change; a file-based router would add a build
 * plugin and a directory layout to say the same thing as twenty lines
 * of code.
 *
 * The history is a hash one. The frontend is served from a custom
 * protocol, where the path of the document is not the path of the
 * application — a deep link would ask the protocol for a file that does
 * not exist. The hash avoids the problem entirely.
 */

import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";

import { AppShell } from "./components/AppShell";
import { HomeRoute } from "./routes/home";
import { InstancesRoute } from "./routes/instances";
import { PerformanceRoute } from "./routes/performance";
import { SettingsRoute } from "./routes/settings";
import { VersionsRoute } from "./routes/versions";

/**
 * The root of the route tree.
 *
 * The shell is the root, so that the frame is mounted once and the
 * screens are swapped inside it. The player must not see the sidebar
 * blink when they move from one screen to another.
 */
const rootRoute = createRootRoute({
  component: AppShell,
});

/** The screen the launcher opens on. */
const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: HomeRoute,
});

/** The list of the versions. */
const versionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/versions",
  component: VersionsRoute,
});

/** The list of the instances. */
const instancesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/instances",
  component: InstancesRoute,
});

/** The settings of the launcher. */
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsRoute,
});

/** The measurements of the benchmark harness. */
const performanceRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/performance",
  component: PerformanceRoute,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  versionsRoute,
  instancesRoute,
  settingsRoute,
  performanceRoute,
]);

/** The router of the launcher. */
export const router = createRouter({
  routeTree,
  history: createHashHistory(),
  defaultPreload: "intent",
});

/** The router has to know the types of itself, or the links are not typed. */
declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
