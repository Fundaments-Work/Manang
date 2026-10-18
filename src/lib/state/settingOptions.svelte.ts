/**
 * The choices a choice (`enum`) setting offers in Settings. Most come from the
 * manifest; some are known only to the plugin at run time, such as the
 * spelling dictionaries the writing plugin finds.
 */

import type { SettingDef } from "$lib/kernel/api";
import { declaredOptions, type SettingOption } from "$lib/workbench/settingOptions";

import { dictionary } from "./dictionary.svelte";

/** Choices reported by a plugin, keyed by setting id. */
const LIVE: Readonly<Record<string, (def: SettingDef) => SettingOption[] | undefined>> =
  {
    "writing.spell-language": (def) => dictionary.languageOptions(def.values),
  };

/** The options `def`'s control shows, with readable labels. */
export function choicesFor(def: SettingDef): SettingOption[] {
  return LIVE[def.id]?.(def) ?? declaredOptions(def.id, def.values);
}
