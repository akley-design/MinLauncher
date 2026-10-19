/**
 * The Russian plural of a number.
 *
 * Russian has three plural forms, and picking the wrong one is the kind
 * of mistake that makes an interface feel foreign.
 *
 * The rule is two steps: the last two digits decide between "many" and
 * the rest, and then the last digit picks the form.
 */
export function plural(count: number, one: string, few: string, many: string): string {
  const mod100 = count % 100;
  if (mod100 >= 11 && mod100 <= 14) {
    return many;
  }

  const mod10 = count % 10;
  if (mod10 === 1) {
    return one;
  }
  if (mod10 >= 2 && mod10 <= 4) {
    return few;
  }
  return many;
}
