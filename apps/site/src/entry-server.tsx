import { renderToString } from "react-dom/server";
import { PrivateShell, Site } from "./site";
import { isPrivateRoute, STATIC_ROUTES } from "./page-meta";
import { renderFeed, renderHead, renderSitemap } from "./seo";

export const routes = STATIC_ROUTES.map((path) => ({
  path,
  private: isPrivateRoute(path),
}));

export function renderRoute(pathname: string) {
  const privateRoute = isPrivateRoute(pathname);
  const html = renderToString(
    privateRoute ? (
      <PrivateShell pathname={pathname} />
    ) : (
      <Site pathname={pathname} />
    ),
  );
  return {
    html,
    head: renderHead(pathname),
    private: privateRoute,
  };
}

export { renderFeed, renderSitemap };
