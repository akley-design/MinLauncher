const RELEASE_AT = Date.parse("2026-10-27T22:04:21.000Z");
const INSTALLER_PATH = "/MinLauncher_0.1.1-beta_x64-setup.exe";

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === "/api/clock.json") {
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

    if (url.pathname === INSTALLER_PATH && Date.now() < RELEASE_AT) {
      return new Response(request.method === "HEAD" ? null : "Beta release is scheduled.", {
        status: 404,
        headers: {
          "Cache-Control": "no-store",
          "Retry-After": "60",
          "X-Content-Type-Options": "nosniff",
        },
      });
    }

    return env.ASSETS.fetch(request);
  },
};
