/**
 * The spelling side of the writing plugin, for Settings: the dictionaries it
 * can check with (`writing.spell.languages`) and the personal dictionary
 * (`writing.spell.words`, `writing.spell.remove-word`).
 *
 * Removing a word makes it a misspelling again, so the open editor drops its
 * cached spelling answers and checks again (the same refresh Add to dictionary
 * does in the editor). Documents not on screen check afresh when shown.
 */

import { recheckWriting } from "$lib/editor/writing";
import { errorMessage, kernel, reportError } from "$lib/kernel";
import { writingApi, type SpellLanguage, type WritingApi } from "$lib/kernel/api";
import {
  spellLanguageOptions,
  type SettingOption,
} from "$lib/workbench/settingOptions";

import { documents } from "./documents.svelte";
import { notice } from "./notice.svelte";

export class DictionaryState {
  words = $state.raw<readonly string[]>([]);
  languages = $state.raw<readonly SpellLanguage[]>([]);
  loaded = $state(false);
  error = $state("");
  /** Words being removed, so their buttons cannot be pressed twice. */
  removing = $state.raw<readonly string[]>([]);

  readonly #api: WritingApi;
  readonly #refresh: () => void;

  constructor(api: WritingApi, refresh: () => void) {
    this.#api = api;
    this.#refresh = refresh;
  }

  /** Read the word list and the languages again. */
  async load(): Promise<void> {
    try {
      const [words, languages] = await Promise.all([
        this.#api.words(),
        this.#api.languages(),
      ]);
      this.words = words.words;
      this.languages = languages.languages;
      this.error = "";
    } catch (error) {
      reportError("the personal dictionary could not be read", error);
      this.error = `The writing plugin did not answer: ${errorMessage(error)}`;
    } finally {
      this.loaded = true;
    }
  }

  /** Take `word` out of the personal dictionary; it is flagged again. */
  async remove(word: string): Promise<void> {
    if (this.removing.includes(word)) return;
    this.removing = [...this.removing, word];
    try {
      await this.#api.removeWord(word);
      this.words = this.words.filter((w) => w !== word);
      this.#refresh();
      notice.show(`Removed “${word}” from the personal dictionary.`);
    } catch (error) {
      reportError(`could not remove "${word}" from the personal dictionary`, error);
      notice.error(`Could not remove “${word}”: ${errorMessage(error)}`);
    } finally {
      this.removing = this.removing.filter((w) => w !== word);
    }
  }

  /**
   * Choices for `writing.spell-language`: the dictionaries the plugin reports
   * that the setting's schema accepts, or undefined before they are known.
   */
  languageOptions(allowed: readonly string[] | undefined): SettingOption[] | undefined {
    return spellLanguageOptions(this.languages, allowed);
  }
}

export const dictionary = new DictionaryState(writingApi(kernel), () => {
  if (documents.view !== undefined) recheckWriting(documents.view);
});
