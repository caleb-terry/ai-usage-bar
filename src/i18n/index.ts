// Minimal i18n layer. One shipped locale (English) plus a resolver that honors
// the user's `language` setting, falling back to the OS locale when set to
// `System`. The structure is the extension point: adding a locale means adding
// a `Strings` record and a case in `resolve`.

import { Language } from "../api";
import { en, Strings } from "./en";

export type { Strings } from "./en";

const locales: Record<string, Strings> = {
  en,
};

/// Resolve the active locale's strings from the stored preference. `System`
/// reads `navigator.language`; anything we don't ship falls back to English.
export function strings(language: Language): Strings {
  const code =
    language === "system"
      ? navigator.language.slice(0, 2).toLowerCase()
      : language;
  return locales[code] ?? en;
}
