import { useSyncExternalStore } from "react";

/**
 * Accent colour: one user-chosen hex turned into the five accent tokens the UI uses, for
 * light and dark themes. Lightness is adjusted so text and buttons keep WCAG AA contrast
 * (4.5:1) whatever colour is picked; the hue and saturation stay the user's.
 */

export const DEFAULT_ACCENT = "#ea580c";

export const ACCENT_PRESETS: { name: string; hex: string }[] = [
  { name: "Orange", hex: DEFAULT_ACCENT },
  { name: "Amber", hex: "#d97706" },
  { name: "Red", hex: "#dc2626" },
  { name: "Rose", hex: "#e11d48" },
  { name: "Pink", hex: "#db2777" },
  { name: "Violet", hex: "#7c3aed" },
  { name: "Indigo", hex: "#4f46e5" },
  { name: "Blue", hex: "#2563eb" },
  { name: "Sky", hex: "#0284c7" },
  { name: "Teal", hex: "#0d9488" },
  { name: "Emerald", hex: "#059669" },
  { name: "Slate", hex: "#475569" },
];

const ACCENT_KEY = "intalix.accent";
/** Precomputed tokens for both themes, read by public/theme-init.js before first paint. */
const VARS_KEY = "intalix.accent.vars";

type Rgb = [number, number, number];
type Hsl = [number, number, number];
export type AccentVars = Record<
  "--primary" | "--primary-hover" | "--primary-foreground" | "--primary-soft" | "--primary-text",
  string
>;

/** Backgrounds the accent must stay readable on (see globals.css). */
const LIGHT_BG: Rgb = [255, 255, 255];
const DARK_BG: Rgb = [11, 11, 13];
const AA = 4.5;

/**
 * Normalises user input to "#rrggbb", or null if it is not a colour.
 *
 * @example
 * normalizeHex("F60")      // => "#ff6600"
 * normalizeHex("#2563EB")  // => "#2563eb"
 * normalizeHex("orange")   // => null
 */
export function normalizeHex(input: string): string | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(input.trim());
  if (!m?.[1]) return null;
  const h = m[1].toLowerCase();
  return `#${h.length === 3 ? [...h].map((c) => c + c).join("") : h}`;
}

const toRgb = (hex: string): Rgb => {
  const n = Number.parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
};
const toHex = ([r, g, b]: Rgb) => `#${[r, g, b].map((v) => Math.round(v).toString(16).padStart(2, "0")).join("")}`;

function rgbToHsl([r, g, b]: Rgb): Hsl {
  const [rn, gn, bn] = [r / 255, g / 255, b / 255];
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l * 100];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === rn ? (gn - bn) / d + (gn < bn ? 6 : 0) : max === gn ? (bn - rn) / d + 2 : (rn - gn) / d + 4;
  return [h * 60, s * 100, l * 100];
}

function hslToRgb([h, s, l]: Hsl): Rgb {
  const sn = s / 100;
  const ln = l / 100;
  const k = (n: number) => (n + h / 30) % 12;
  const a = sn * Math.min(ln, 1 - ln);
  const f = (n: number) => ln - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  return [f(0) * 255, f(8) * 255, f(4) * 255];
}

function luminance([r, g, b]: Rgb): number {
  const c = (v: number) => {
    const x = v / 255;
    return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * c(r) + 0.7152 * c(g) + 0.0722 * c(b);
}

/**
 * WCAG contrast ratio between two colours (1 to 21).
 *
 * @example
 * contrast([255, 255, 255], [0, 0, 0]) // => 21
 */
export function contrast(a: Rgb, b: Rgb): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p) as [number, number];
  return (x + 0.05) / (y + 0.05);
}

/** Moves lightness one step at a time until `ok` holds (or lightness runs out). */
function shiftUntil(hsl: Hsl, direction: 1 | -1, ok: (rgb: Rgb) => boolean): Hsl {
  let [h, s, l] = hsl;
  while (!ok(hslToRgb([h, s, l])) && l > 0 && l < 100) l = Math.min(100, Math.max(0, l + direction));
  return [h, s, l];
}

const shift = ([h, s, l]: Hsl, by: number): Hsl => [h, s, Math.min(100, Math.max(0, l + by))];
const rgba = ([r, g, b]: Rgb, alpha: number) => `rgb(${Math.round(r)} ${Math.round(g)} ${Math.round(b)} / ${alpha})`;

/**
 * Derives the accent tokens for one theme.
 *
 * Light: buttons darken until white text on them reaches 4.5:1; accent text darkens until
 * it reaches 4.5:1 on white. Dark: buttons lighten until they reach 4.5:1 against the dark
 * background (their text is near-black); accent text lightens a little further.
 */
export function deriveAccent(hex: string, theme: "light" | "dark"): AccentVars {
  const base = rgbToHsl(toRgb(hex));
  if (theme === "light") {
    const primary = shiftUntil(base, -1, (c) => contrast(c, LIGHT_BG) >= AA);
    const text = shiftUntil(primary, -1, (c) => contrast(c, LIGHT_BG) >= AA + 0.3);
    return {
      "--primary": toHex(hslToRgb(primary)),
      "--primary-hover": toHex(hslToRgb(shift(primary, -6))),
      "--primary-foreground": "#ffffff",
      "--primary-soft": rgba(hslToRgb(base), 0.08),
      "--primary-text": toHex(hslToRgb(text)),
    };
  }
  const primary = shiftUntil(base, 1, (c) => contrast(c, DARK_BG) >= AA + 1.5);
  const text = shiftUntil(primary, 1, (c) => contrast(c, DARK_BG) >= AA + 2.5);
  // Near-black tinted with the hue, for text on the bright dark-theme buttons.
  const darkInk = hslToRgb([base[0], Math.min(base[1], 60), 6]);
  return {
    "--primary": toHex(hslToRgb(primary)),
    "--primary-hover": toHex(hslToRgb(shift(primary, 6))),
    "--primary-foreground": toHex(darkInk),
    "--primary-soft": rgba(hslToRgb(primary), 0.1),
    "--primary-text": toHex(hslToRgb(text)),
  };
}

/* ---------------------------------------------------------------------------------------- */
/* Store                                                                                     */
/* ---------------------------------------------------------------------------------------- */

function readAccent(): string {
  try {
    return normalizeHex(localStorage.getItem(ACCENT_KEY) ?? "") ?? DEFAULT_ACCENT;
  } catch {
    return DEFAULT_ACCENT;
  }
}

let current = readAccent();
const listeners = new Set<() => void>();

/**
 * Writes the accent tokens for the given theme onto <html>. The default orange keeps its
 * hand-tuned values from globals.css, so nothing is set inline for it.
 */
export function applyAccent(theme: "light" | "dark") {
  const style = document.documentElement.style;
  const vars = current === DEFAULT_ACCENT ? null : deriveAccent(current, theme);
  for (const key of ["--primary", "--primary-hover", "--primary-foreground", "--primary-soft", "--primary-text"]) {
    const value = vars?.[key as keyof AccentVars];
    if (value) style.setProperty(key, value);
    else style.removeProperty(key);
  }
}

export function setAccent(hex: string) {
  const next = normalizeHex(hex);
  if (!next) return;
  current = next;
  try {
    localStorage.setItem(ACCENT_KEY, next);
    if (next === DEFAULT_ACCENT) localStorage.removeItem(VARS_KEY);
    else
      localStorage.setItem(
        VARS_KEY,
        JSON.stringify({ light: deriveAccent(next, "light"), dark: deriveAccent(next, "dark") }),
      );
  } catch {
    // The colour still applies for this session.
  }
  applyAccent(document.documentElement.dataset.theme === "light" ? "light" : "dark");
  listeners.forEach((l) => l());
}

export function useAccent(): string {
  return useSyncExternalStore(
    (onChange) => {
      listeners.add(onChange);
      return () => listeners.delete(onChange);
    },
    () => current,
  );
}
