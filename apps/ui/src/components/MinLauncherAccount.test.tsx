import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  onOpenUrl: vi.fn(),
  getCurrent: vi.fn(),
  open: vi.fn(),
  signInWithOAuth: vi.fn(),
  exchangeCodeForSession: vi.fn(),
  onAuthStateChange: vi.fn(),
  getSession: vi.fn(),
  unsubscribe: vi.fn(),
  deepLinkHandler: null as null | ((urls: string[]) => void),
  authStateHandler: null as null | ((event: string, session: {
    user: { email: string; user_metadata: Record<string, unknown> };
  } | null) => void),
}));

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/plugin-deep-link", () => ({
  getCurrent: mocks.getCurrent,
  onOpenUrl: mocks.onOpenUrl,
}));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: mocks.open }));
vi.mock("../auth", () => ({
  launcherAuth: {
    auth: {
      onAuthStateChange: mocks.onAuthStateChange,
      getSession: mocks.getSession,
      signInWithOAuth: mocks.signInWithOAuth,
      exchangeCodeForSession: mocks.exchangeCodeForSession,
    },
  },
  parseMinLauncherUrl: (raw: string) => {
    const url = new URL(raw);
    const error = url.searchParams.get("error_description") ?? url.searchParams.get("error");
    if (error) return { kind: "error", message: error };
    const code = url.searchParams.get("code");
    return code ? { kind: "oauth", code } : null;
  },
}));

import { MinLauncherAccount } from "./MinLauncherAccount";

describe("MinLauncherAccount OAuth return", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.deepLinkHandler = null;
    mocks.authStateHandler = null;
    mocks.onOpenUrl.mockImplementation(async (handler) => {
      mocks.deepLinkHandler = handler;
      return mocks.unsubscribe;
    });
    mocks.getCurrent.mockResolvedValue(null);
    mocks.open.mockResolvedValue(undefined);
    mocks.signInWithOAuth.mockResolvedValue({ data: { url: "https://auth.example/oauth" }, error: null });
    mocks.exchangeCodeForSession.mockResolvedValue({ error: null });
    mocks.onAuthStateChange.mockImplementation((handler) => {
      mocks.authStateHandler = (event, session) => handler(event, session);
      return { data: { subscription: { unsubscribe: mocks.unsubscribe } } };
    });
    mocks.getSession.mockResolvedValue({ data: { session: null }, error: null });
  });

  it("clears browser waiting and allows retry when the provider returns an OAuth error", async () => {
    render(<MinLauncherAccount />);
    fireEvent.click(screen.getByRole("button", { name: "Войти через Google" }));
    expect(await screen.findByText(/Ожидаем подтверждение в браузере/)).toBeInTheDocument();

    await waitFor(() => expect(mocks.deepLinkHandler).toBeTypeOf("function"));
    await act(async () => {
      mocks.deepLinkHandler?.([
        "minlauncher://auth/callback?error=access_denied&error_description=Google%20отклонил%20вход",
      ]);
    });

    expect(await screen.findByRole("alert")).toHaveTextContent("Google отклонил вход");
    await waitFor(() => expect(screen.queryByText(/Ожидаем подтверждение в браузере/)).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Войти через Google" })).toBeEnabled();
  });

  it("shows PKCE exchange errors and lets the user retry", async () => {
    mocks.exchangeCodeForSession.mockResolvedValue({ error: new Error("PKCE-код устарел") });
    render(<MinLauncherAccount />);
    fireEvent.click(screen.getByRole("button", { name: "Войти через Google" }));
    expect(await screen.findByText(/Ожидаем подтверждение в браузере/)).toBeInTheDocument();

    await waitFor(() => expect(mocks.deepLinkHandler).toBeTypeOf("function"));
    await act(async () => {
      mocks.deepLinkHandler?.(["minlauncher://auth/callback?code=pkce-code"]);
    });

    expect(await screen.findByRole("alert")).toHaveTextContent("PKCE-код устарел");
    await waitFor(() => expect(screen.queryByText(/Ожидаем подтверждение в браузере/)).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Войти через Google" })).toBeEnabled();
  });

  it("shows the account and ends browser waiting after a successful PKCE exchange", async () => {
    render(<MinLauncherAccount />);
    fireEvent.click(screen.getByRole("button", { name: "Войти через Google" }));
    expect(await screen.findByText(/Ожидаем подтверждение в браузере/)).toBeInTheDocument();

    await waitFor(() => expect(mocks.deepLinkHandler).toBeTypeOf("function"));
    await act(async () => {
      mocks.deepLinkHandler?.(["minlauncher://auth/callback?code=pkce-code"]);
    });

    await waitFor(() => expect(mocks.exchangeCodeForSession).toHaveBeenCalledWith("pkce-code"));
    await act(async () => {
      mocks.authStateHandler?.("SIGNED_IN", {
        user: { email: "player@example.com", user_metadata: { display_name: "Player" } },
      });
    });

    expect(await screen.findByText("player@example.com")).toBeInTheDocument();
    expect(screen.queryByText(/Ожидаем подтверждение в браузере/)).not.toBeInTheDocument();
  });

  it("registers the URL listener before reading a cold-start deep link", async () => {
    let finishRegistration: ((unlisten: () => void) => void) | undefined;
    mocks.onOpenUrl.mockImplementation(() => new Promise((resolve) => {
      finishRegistration = resolve;
    }));
    render(<MinLauncherAccount />);

    await waitFor(() => expect(finishRegistration).toBeTypeOf("function"));
    expect(mocks.getCurrent).not.toHaveBeenCalled();
    finishRegistration?.(mocks.unsubscribe);
    await waitFor(() => expect(mocks.getCurrent).toHaveBeenCalledOnce());
  });
});
