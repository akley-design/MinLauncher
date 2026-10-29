export type RegistrationFields = {
  nickname: string;
  email: string;
  password: string;
  repeatPassword: string;
};

export const nicknamePattern = /^[A-Za-z0-9_]{3,16}$/;
export const nicknamePatternSource = "[A-Za-z0-9_]{3,16}";
const emailPattern = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const passwordBytes = (password: string) =>
  new TextEncoder().encode(password).length;
const passwordLength = (password: string) => Array.from(password).length;

export function validateRegistration(
  fields: RegistrationFields,
): string | null {
  if (!emailPattern.test(fields.email.trim())) {
    return "Проверьте адрес электронной почты.";
  }
  if (!nicknamePattern.test(fields.nickname.trim())) {
    return "Ник должен содержать 3–16 латинских букв, цифр или символов _.";
  }
  if (passwordLength(fields.password) < 12) {
    return "Пароль должен содержать не менее 12 символов.";
  }
  if (passwordBytes(fields.password) > 72) {
    return "Пароль не должен превышать 72 байта.";
  }
  if (fields.password !== fields.repeatPassword) {
    return "Пароли не совпадают.";
  }
  return null;
}

export function validateNewPassword(
  password: string,
  repeatPassword: string,
): string | null {
  if (passwordLength(password) < 12)
    return "Пароль должен содержать не менее 12 символов.";
  if (passwordBytes(password) > 72)
    return "Пароль не должен превышать 72 байта.";
  if (password !== repeatPassword) return "Пароли не совпадают.";
  return null;
}
