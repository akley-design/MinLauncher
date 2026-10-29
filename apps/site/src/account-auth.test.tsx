// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  signUp: vi.fn(),
  resend: vi.fn(),
}));

vi.mock("./supabase", () => ({
  supabase: {
    functions: { invoke: mocks.invoke },
    auth: { signUp: mocks.signUp, resend: mocks.resend },
  },
}));

import { AuthPage } from "./account-auth";

describe("registration confirmation email feedback", () => {
  beforeEach(() => {
    mocks.invoke.mockResolvedValue({ data: { available: true }, error: null });
    mocks.signUp.mockResolvedValue({ data: { session: null }, error: null });
    mocks.resend.mockResolvedValue({ error: null });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  async function register() {
    render(<AuthPage session={null} />);
    fireEvent.click(screen.getByRole("button", { name: "Регистрация" }));
    fireEvent.change(screen.getByLabelText("НИК MINECRAFT"), {
      target: { value: "Player_1" },
    });
    fireEvent.change(screen.getByLabelText("ЭЛЕКТРОННАЯ ПОЧТА"), {
      target: { value: "player@example.com" },
    });
    fireEvent.change(screen.getByLabelText("ПАРОЛЬ"), {
      target: { value: "long-enough-password" },
    });
    fireEvent.change(screen.getByLabelText("ПОВТОРИТЕ ПАРОЛЬ"), {
      target: { value: "long-enough-password" },
    });
    fireEvent.click(screen.getByRole("button", { name: "СОЗДАТЬ АККАУНТ" }));
  }

  it("shows the confirmation notice and resend action after successful signup", async () => {
    await register();

    expect((await screen.findByRole("status")).textContent).toContain(
      "Проверьте почту",
    );
    expect(
      screen.getByRole("button", {
        name: "Отправить письмо подтверждения ещё раз",
      }),
    ).toBeTruthy();
  });

  it("does not claim success when signup fails and keeps resend available", async () => {
    mocks.signUp.mockResolvedValue({
      data: { session: null },
      error: { code: "email_address_not_authorized", status: 400 },
    });
    await register();

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Отправка на этот адрес пока не настроена",
    );
    expect(screen.queryByRole("status")).toBeNull();
    expect(
      screen.getByRole("button", {
        name: "Отправить письмо подтверждения ещё раз",
      }),
    ).toBeTruthy();
  });

  it("does not expose a thrown SMTP error from signup", async () => {
    mocks.signUp.mockRejectedValue(new Error("private SMTP response"));
    await register();

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Не удалось создать аккаунт",
    );
    expect(screen.queryByText("private SMTP response")).toBeNull();
  });

  it("reports resend success without revealing whether an account exists", async () => {
    await register();
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Отправить письмо подтверждения ещё раз",
      }),
    );

    expect((await screen.findByRole("status")).textContent).toContain(
      "Если адрес можно подтвердить, письмо отправлено повторно.",
    );
    expect(mocks.resend).toHaveBeenCalledWith({
      type: "signup",
      email: "player@example.com",
      options: { emailRedirectTo: `${window.location.origin}/auth/callback` },
    });
  });

  it("keeps resend available and reports errors after a failed resend", async () => {
    mocks.resend.mockResolvedValue({
      error: { code: "over_email_send_rate_limit", status: 429 },
    });
    await register();
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Отправить письмо подтверждения ещё раз",
      }),
    );

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Слишком много попыток отправки",
    );
    expect(
      screen.getByRole("button", {
        name: "Отправить письмо подтверждения ещё раз",
      }),
    ).toBeTruthy();
  });

  it("notifies the user if an account with this email already exists", async () => {
    mocks.signUp.mockResolvedValue({
      data: { session: null, user: { identities: [] } },
      error: null,
    });
    await register();

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Пользователь с такой почтой уже зарегистрирован",
    );
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("informs user about SMTP misconfiguration when email confirmation fails", async () => {
    mocks.signUp.mockResolvedValue({
      data: null,
      error: { message: "Error sending confirmation email", status: 500 },
    });
    await register();

    expect((await screen.findByRole("alert")).textContent).toContain(
      "В Supabase не настроен SMTP-сервер",
    );
  });
});
