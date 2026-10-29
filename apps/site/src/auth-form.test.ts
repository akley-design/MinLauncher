import { describe, expect, it } from "vitest";
import { validateRegistration } from "./auth-form";

describe("validateRegistration", () => {
  it("requires a real email-shaped address", () => {
    expect(
      validateRegistration({
        nickname: "Player_1",
        email: "nope",
        password: "long-enough-password",
        repeatPassword: "long-enough-password",
      }),
    ).toBe("Проверьте адрес электронной почты.");
  });

  it("requires a Minecraft nickname from the supported character set", () => {
    expect(
      validateRegistration({
        nickname: "Player name",
        email: "player@example.com",
        password: "long-enough-password",
        repeatPassword: "long-enough-password",
      }),
    ).toBe("Ник должен содержать 3–16 латинских букв, цифр или символов _.");
  });

  it("rejects weak and mismatched passwords", () => {
    expect(
      validateRegistration({
        nickname: "Player_1",
        email: "player@example.com",
        password: "short",
        repeatPassword: "short",
      }),
    ).toBe("Пароль должен содержать не менее 12 символов.");
    expect(
      validateRegistration({
        nickname: "Player_1",
        email: "player@example.com",
        password: "long-enough-password",
        repeatPassword: "different-password",
      }),
    ).toBe("Пароли не совпадают.");
  });

  it("limits UTF-8 password size to the Supabase bcrypt limit", () => {
    expect(
      validateRegistration({
        nickname: "Player_1",
        email: "player@example.com",
        password: "я".repeat(37),
        repeatPassword: "я".repeat(37),
      }),
    ).toBe("Пароль не должен превышать 72 байта.");
  });

  it("accepts a valid registration", () => {
    expect(
      validateRegistration({
        nickname: "Player_1",
        email: "player@example.com",
        password: "long-enough-password",
        repeatPassword: "long-enough-password",
      }),
    ).toBeNull();
  });
});
