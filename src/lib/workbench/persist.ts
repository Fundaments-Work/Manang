/**
 * Layout and session persistence.
 *
 * Browser storage, always wrapped: it throws in private windows and when site
 * data is blocked, and the workbench must start either way. This is a stand-in:
 * the settings plugin will own persisted preferences, and only this module
 * needs to change when it lands.
 */

export const LAYOUT_KEY = "manang.layout.v1";
export const SESSION_KEY = "manang.session.v1";

export function loadJson(key: string): unknown {
  try {
    const raw = window.localStorage.getItem(key);
    return raw === null ? null : (JSON.parse(raw) as unknown);
  } catch {
    return null;
  }
}

export function saveJson(key: string, value: unknown): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* Storage is unavailable; the workbench simply will not remember. */
  }
}

export function clearKey(key: string): void {
  try {
    window.localStorage.removeItem(key);
  } catch {
    /* See above. */
  }
}
