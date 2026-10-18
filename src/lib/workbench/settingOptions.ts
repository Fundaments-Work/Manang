/**
 * How a choice setting's values read in Settings. Manifest values are ids
 * (`apa`, `en_US`); people should see names. Pure, so it is unit-tested.
 */

import type { CitationStyle, SpellLanguage } from "$lib/kernel/api";

import { formatCategoryLabel } from "./settings";

/** A choice for a setting: its stored value and what the control shows. */
export interface SettingOption {
  readonly value: string;
  readonly label: string;
}

/** Labels for values whose id is not a word, keyed by setting id. */
const VALUE_LABELS: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  "citations.style": { apa: "APA", numeric: "Numeric (IEEE)" },
};

let languageNames: Intl.DisplayNames | null | undefined;

function displayNames(): Intl.DisplayNames | null {
  if (languageNames === undefined) {
    try {
      languageNames = new Intl.DisplayNames(["en"], { type: "language" });
    } catch {
      languageNames = null;
    }
  }
  return languageNames;
}

/** "en_US" -> "American English"; the code itself when it has no name. */
export function languageName(code: string): string {
  const tag = code.replace(/_/g, "-");
  try {
    const name = displayNames()?.of(tag);
    if (name !== undefined && name !== "" && name.toLowerCase() !== tag.toLowerCase())
      return name;
  } catch {
    // Not a valid BCP 47 tag (a user dictionary named freely): show the code.
  }
  return code;
}

/** A spelling dictionary as a choice: its language, and where it comes from. */
export function spellLanguageLabel(language: SpellLanguage): string {
  const name = languageName(language.code);
  return language.source === "user" ? `${name} (your dictionary)` : name;
}

/** The label of one value of a choice setting. */
export function optionLabel(settingId: string, value: string): string {
  const known = VALUE_LABELS[settingId]?.[value];
  if (known !== undefined) return known;
  if (settingId === "writing.spell-language") return languageName(value);
  return formatCategoryLabel(value);
}

/** The `citations.style` value as a style `citations.format` accepts. */
export function citationStyleOf(value: unknown): CitationStyle | undefined {
  return value === "apa" || value === "numeric" || value === "ieee" ? value : undefined;
}

/** A choice setting's declared values as options. */
export function declaredOptions(
  settingId: string,
  values: readonly string[] | undefined,
): SettingOption[] {
  return (values ?? []).map((value) => ({
    value,
    label: optionLabel(settingId, value),
  }));
}

/**
 * Dictionaries the writing plugin reports, limited to the values the setting's
 * schema accepts (a value outside it would be refused when saved). Undefined
 * when none are known, so the control falls back to the declared values.
 */
export function spellLanguageOptions(
  languages: readonly SpellLanguage[],
  allowed: readonly string[] | undefined,
): SettingOption[] | undefined {
  const options = languages
    .filter((l) => allowed === undefined || allowed.includes(l.code))
    .map((l) => ({ value: l.code, label: spellLanguageLabel(l) }));
  return options.length > 0 ? options : undefined;
}
