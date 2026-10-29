const RELEASE_AT = Date.parse("2026-10-27T22:04:21.000Z");
const INSTALLER_PATH = "/MinLauncher_0.1.1-beta_x64-setup.exe";
const HTML_SECURITY = {
  "X-Content-Type-Options": "nosniff",
  "X-Frame-Options": "DENY",
  "Referrer-Policy": "strict-origin-when-cross-origin",
  "Permissions-Policy": "camera=(), microphone=(), geolocation=()",
  "Content-Security-Policy":
    "default-src 'self'; img-src 'self' data:; style-src 'self'; font-src 'self' data:; script-src 'self'; connect-src 'self' https: wss:; base-uri 'self'; form-action 'self'; frame-ancestors 'none'",
  "Strict-Transport-Security": "max-age=31536000; includeSubDomains",
};

function isAssetPath(pathname) {
  return /\.[a-z0-9]+$/i.test(pathname);
}

function assetCache(pathname) {
  if (pathname.startsWith("/assets/"))
    return "public, max-age=31536000, immutable";
  if (
    pathname.startsWith("/fonts/") ||
    pathname.startsWith("/media/") ||
    pathname.startsWith("/og/")
  )
    return "public, max-age=604800";
  return "no-cache";
}

function withHeaders(response, headers, status = response.status) {
  const next = new Headers(response.headers);
  for (const [key, value] of Object.entries(headers)) next.set(key, value);
  return new Response(response.body, {
    status,
    statusText: response.statusText,
    headers: next,
  });
}

function assetRequest(request, pathname, fresh = false) {
  const url = new URL(request.url);
  url.pathname = pathname;
  url.search = "";
  const headers = new Headers(request.headers);
  if (fresh) {
    headers.delete("if-none-match");
    headers.delete("if-modified-since");
  }
  return new Request(url, {
    method: request.method,
    headers,
    redirect: request.redirect,
  });
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const pathname = url.pathname;

    if (pathname === "/api/clock.json") {
      return Response.json(
        { now: Date.now(), releaseAt: RELEASE_AT },
        {
          headers: {
            "Cache-Control": "no-store, no-cache, must-revalidate",
            "Content-Type": "application/json; charset=utf-8",
            "X-Content-Type-Options": "nosniff",
          },
        },
      );
    }

    if (pathname === INSTALLER_PATH && Date.now() < RELEASE_AT) {
      return new Response(
        request.method === "HEAD" ? null : "Beta release is scheduled.",
        {
          status: 404,
          headers: {
            "Cache-Control": "no-store",
            "Retry-After": "60",
            "X-Content-Type-Options": "nosniff",
          },
        },
      );
    }

    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response("Method not allowed", {
        status: 405,
        headers: { Allow: "GET, HEAD", "X-Content-Type-Options": "nosniff" },
      });
    }

    if (pathname === "/index.html") {
      return new Response(null, {
        status: 308,
        headers: { Location: `/${url.search}`, ...HTML_SECURITY },
      });
    }
    if (pathname !== "/" && pathname.endsWith("/")) {
      return new Response(null, {
        status: 308,
        headers: {
          Location: `${pathname.replace(/\/+$/, "")}${url.search}`,
          ...HTML_SECURITY,
        },
      });
    }

    if (pathname !== "/" && isAssetPath(pathname)) {
      const response = await env.ASSETS.fetch(request);
      return withHeaders(response, {
        "Cache-Control": assetCache(pathname),
        "X-Content-Type-Options": "nosniff",
        "Strict-Transport-Security": "max-age=31536000; includeSubDomains",
      });
    }

    const page = await env.ASSETS.fetch(assetRequest(request, pathname, true));
    if (page.status === 200) {
      return withHeaders(page, {
        ...HTML_SECURITY,
        "Cache-Control": "no-cache",
      });
    }

    const isNotFoundHtml = (page.headers.get("content-type") ?? "").includes(
      "text/html",
    );
    const notFound = isNotFoundHtml
      ? page
      : await env.ASSETS.fetch(assetRequest(request, "/404", true));
    return withHeaders(
      notFound,
      {
        ...HTML_SECURITY,
        "Cache-Control": "no-cache",
        "X-Robots-Tag": "noindex",
      },
      404,
    );
  },
};
