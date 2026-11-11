// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import type { SupabaseClient } from "@supabase/supabase-js";
import { afterEach, describe, expect, it, vi } from "vitest";

import { TelegramLinkCard } from "./telegram-link";

const BOT_USERNAME = "MinLauncherSupportBot";
const PAYLOAD = "a".repeat(43);

function makeClient(invoke: ReturnType<typeof vi.fn>) {
  return { functions: { invoke } } as unknown as SupabaseClient;
}

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("Telegram account link card", () => {
  it("asks the signed-out visitor to sign in without calling the API", () => {
    const invoke = vi.fn();

    render(
      <TelegramLinkCard
        client={makeClient(invoke)}
        userId={null}
        signedIn={false}
        botUsername={BOT_USERNAME}
      />,
    );

    expect(screen.getByText(/войдите в аккаунт MinLauncher/i)).toBeTruthy();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("creates a short-lived deep link and lets the visitor refresh status", async () => {
    const invoke = vi.fn(
      async (_name: string, { body }: { body: { action: string } }) => {
        if (body.action === "status")
          return { data: { linked: false }, error: null };
        return {
          data: { payload: PAYLOAD, expiresAt: "2099-01-01T00:10:00.000Z" },
          error: null,
        };
      },
    );

    render(
      <TelegramLinkCard
        client={makeClient(invoke)}
        userId="user-1"
        signedIn
        botUsername={BOT_USERNAME}
      />,
    );
    await settle();

    expect(screen.getByText("Профиль не привязан")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /создать ссылку/i }));
    await settle();

    const link = screen.getByRole("link", { name: /открыть Telegram/i });
    expect(link.getAttribute("href")).toBe(
      `https://t.me/${BOT_USERNAME}?start=link_${PAYLOAD}`,
    );
    expect(invoke.mock.calls.map(([, options]) => options.body.action)).toEqual(
      ["status", "create"],
    );
    expect(
      invoke.mock.calls.every(
        ([, options]) => Object.keys(options.body).join() === "action",
      ),
    ).toBe(true);
    expect(
      screen.getByRole("button", { name: /проверить привязку/i }),
    ).toBeTruthy();
  });

  it("does not expose an expired challenge link and allows creating a fresh one", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2030-01-01T00:20:00.000Z"));
    const invoke = vi.fn(
      async (_name: string, { body }: { body: { action: string } }) => {
        if (body.action === "status")
          return { data: { linked: false }, error: null };
        return {
          data: { payload: PAYLOAD, expiresAt: "2030-01-01T00:10:00.000Z" },
          error: null,
        };
      },
    );

    render(
      <TelegramLinkCard
        client={makeClient(invoke)}
        userId="user-1"
        signedIn
        botUsername={BOT_USERNAME}
      />,
    );
    await settle();
    fireEvent.click(screen.getByRole("button", { name: /создать ссылку/i }));
    await settle();

    expect(
      screen.queryByRole("link", { name: /открыть Telegram/i }),
    ).toBeNull();
    expect(screen.getByText(/срок ссылки истёк/i)).toBeTruthy();
    expect(
      screen.getByRole("button", { name: /создать новую ссылку/i }),
    ).toBeTruthy();
    vi.useRealTimers();
  });

  it("unlinks only after the visitor confirms and keeps the linked status on API failure", async () => {
    let unlinkFails = false;
    const invoke = vi.fn(
      async (_name: string, { body }: { body: { action: string } }) => {
        if (body.action === "status")
          return { data: { linked: true }, error: null };
        if (body.action === "unlink" && unlinkFails)
          return { data: null, error: new Error("private upstream details") };
        return { data: {}, error: null };
      },
    );

    render(
      <TelegramLinkCard
        client={makeClient(invoke)}
        userId="user-1"
        signedIn
        botUsername={BOT_USERNAME}
      />,
    );
    await settle();
    expect(screen.getByText("Профиль привязан")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /отвязать аккаунт/i }));
    expect(
      invoke.mock.calls.some(([, options]) => options.body.action === "unlink"),
    ).toBe(false);
    fireEvent.click(
      screen.getByRole("button", { name: /подтвердить отвязку/i }),
    );
    await settle();
    expect(screen.getByText(/telegram-аккаунт отвязан/i)).toBeTruthy();

    unlinkFails = true;
    fireEvent.click(
      screen.getByRole("button", { name: /проверить привязку/i }),
    );
    await settle();
    fireEvent.click(screen.getByRole("button", { name: /отвязать аккаунт/i }));
    fireEvent.click(
      screen.getByRole("button", { name: /подтвердить отвязку/i }),
    );
    await settle();
    expect(screen.getByText("Профиль привязан")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).not.toContain(
      "private upstream details",
    );
  });

  it("shows a safe message when the challenge conflicts with an existing link", async () => {
    const invoke = vi.fn(
      async (_name: string, { body }: { body: { action: string } }) => {
        if (body.action === "status")
          return { data: { linked: false }, error: null };
        return {
          data: null,
          error: Object.assign(new Error("https://secret.example/token"), {
            context: new Response(JSON.stringify({ error: "already_linked" })),
          }),
        };
      },
    );

    render(
      <TelegramLinkCard
        client={makeClient(invoke)}
        userId="user-1"
        signedIn
        botUsername={BOT_USERNAME}
      />,
    );
    await settle();
    fireEvent.click(screen.getByRole("button", { name: /создать ссылку/i }));
    await settle();

    expect(screen.getByRole("alert").textContent).toMatch(/уже привязан/i);
    expect(screen.getByRole("alert").textContent).not.toContain(
      "secret.example",
    );
  });

  it("supports Ukrainian text and reports missing public bot configuration", async () => {
    const invoke = vi.fn(
      async (_name: string, { body }: { body: { action: string } }) =>
        body.action === "status"
          ? { data: { linked: false }, error: null }
          : { data: null, error: null },
    );
    render(
      <TelegramLinkCard
        client={makeClient(invoke)}
        userId="user-1"
        signedIn
        botUsername="invalid username"
        language="uk"
      />,
    );

    await settle();
    expect(screen.getByText("Профіль не прив’язано")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toMatch(
      /прив’язку Telegram ще не налаштовано/i,
    );
    expect(
      screen
        .getByRole("button", { name: /створити посилання/i })
        .hasAttribute("disabled"),
    ).toBe(true);
  });
});
