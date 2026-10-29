/**
 * The six screens of the launcher.
 *
 * Hash history is not a preference: the frontend runs on a custom
 * protocol, where a path deep link asks for a file that does not exist.
 */

import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import { lazy } from "react";

import { AppShell } from "./components/AppShell";

// These lazy components are route values, not Fast Refresh exports.
/* eslint-disable react-refresh/only-export-components */
const DownloadsRoute = lazy(async () => ({
  default: (await import("./routes/downloads")).DownloadsRoute,
}));
const HomeRoute = lazy(async () => ({
  default: (await import("./routes/home")).HomeRoute,
}));
const InstancesRoute = lazy(async () => ({
  default: (await import("./routes/instances")).InstancesRoute,
}));
const PerformanceRoute = lazy(async () => ({
  default: (await import("./routes/performance")).PerformanceRoute,
}));
const SettingsRoute = lazy(async () => ({
  default: (await import("./routes/settings")).SettingsRoute,
}));
const VersionsRoute = lazy(async () => ({
  default: (await import("./routes/versions")).VersionsRoute,
}));
/* eslint-enable react-refresh/only-export-components */

/** The shell is the root, so the frame mounts once and does not blink on navigation. */
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

/** The queue of the downloads. */
const downloadsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/downloads",
  component: DownloadsRoute,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  versionsRoute,
  instancesRoute,
  downloadsRoute,
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
