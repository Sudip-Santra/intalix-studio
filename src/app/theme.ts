import { useEffect, useSyncExternalStore } from "react";
import { applyAccent } from "./accent";

/** What the user chose. "system" follows the Windows light/dark setting. */
export type ThemePreference = "light" | "dark" | "system";
const STORAGE_KEY = "intalix.theme";

function readPreference(): ThemePreference {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    // Storage unavailable: fall back to the default.
  }
  return "dark";
}

const systemQuery = () => window.matchMedia("(prefers-color-scheme: dark)");
const resolve = (p: ThemePreference) => (p === "system" ? (systemQuery().matches ? "dark" : "light") : p);
const apply = (p: ThemePreference) => {
  const theme = resolve(p);
  document.documentElement.dataset.theme = theme;
  // Custom accents are derived per theme, so they are re-applied whenever the theme changes.
  applyAccent(theme);
};

// One shared value for the whole app, so the settings dialog and the root stay in sync.
let current = readPreference();
const listeners = new Set<() => void>();

export function setThemePreference(next: ThemePreference) {
  current = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // The theme still applies for this session.
  }
  apply(next);
  listeners.forEach((l) => l());
}

export function useThemePreference(): ThemePreference {
  return useSyncExternalStore(
    (onChange) => {
      listeners.add(onChange);
      return () => listeners.delete(onChange);
    },
    () => current,
  );
}

/** The theme actually showing ("system" resolved to light or dark). */
export function useResolvedTheme(): "light" | "dark" {
  return resolve(useThemePreference());
}

/** Mount once at the root: applies the theme and follows Windows when set to "system". */
export function useThemeSync() {
  const preference = useThemePreference();
  useEffect(() => {
    apply(preference);
    if (preference !== "system") return;
    const query = systemQuery();
    const onChange = () => apply("system");
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, [preference]);
}
