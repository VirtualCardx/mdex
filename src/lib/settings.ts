/**
 * Small persistent UI preferences.
 *
 * Values live in the webview's localStorage, which Tauri scopes to the app
 * identifier, so settings survive restarts without extra backend support.
 */

import type { Theme } from "./types";

const THEME_KEY = "mdex.theme";

/**
 * The stored theme choice, falling back to the OS color-scheme preference
 * when nothing has been chosen yet, then to dark.
 */
export function loadTheme(): Theme {
  try {
    const stored = localStorage.getItem(THEME_KEY);
    if (stored === "light" || stored === "dark") return stored;
    if (window.matchMedia?.("(prefers-color-scheme: light)").matches) {
      return "light";
    }
  } catch {
    // localStorage unavailable — fall through to the default.
  }
  return "dark";
}

export function saveTheme(theme: Theme): void {
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    // Write failed (storage disabled) — theme stays session-only.
  }
}
