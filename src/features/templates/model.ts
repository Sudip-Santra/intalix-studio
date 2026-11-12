/**
 * Report template definition (schema version 1).
 *
 * A template is presentation only: page setup, brand, header, table styling, number
 * and date formats, footer and watermark. It never contains SQL, connection details or data,
 * so a report can apply any template to any query result.
 *
 * Stored definitions always pass through `normalizeDefinition`, which fills missing fields and
 * replaces invalid values with defaults. A damaged or older template therefore still opens.
 */
import { z } from "zod";

export const SCHEMA_VERSION = 1;

/* ------------------------------------------------------------------------------------------
 * Option lists (the UI renders these; the schema accepts only these values)
 * ---------------------------------------------------------------------------------------- */

export const PAGE_SIZES = ["A4", "Letter", "Legal"] as const;
/** Page dimensions in millimetres, portrait. */
export const PAGE_MM: Record<PageSize, { w: number; h: number }> = {
  A4: { w: 210, h: 297 },
  Letter: { w: 215.9, h: 279.4 },
  Legal: { w: 215.9, h: 355.6 },
};
export const MARGINS = ["narrow", "normal", "wide"] as const;
export const MARGIN_MM: Record<Margins, number> = { narrow: 7, normal: 11, wide: 16 };

export const FONTS = ["inter", "segoe", "arial", "calibri", "georgia", "cambria"] as const;
export const FONT_META: Record<FontId, { label: string; stack: string }> = {
  inter: { label: "Inter", stack: '"Inter Variable", "Segoe UI", sans-serif' },
  segoe: { label: "Segoe UI", stack: '"Segoe UI", "Inter Variable", sans-serif' },
  arial: { label: "Arial", stack: "Arial, Helvetica, sans-serif" },
  calibri: { label: "Calibri", stack: 'Calibri, "Segoe UI", sans-serif' },
  georgia: { label: "Georgia", stack: "Georgia, Cambria, serif" },
  cambria: { label: "Cambria", stack: "Cambria, Georgia, serif" },
};

export const DATE_FORMATS = ["DD MMM YYYY", "YYYY-MM-DD", "DD/MM/YYYY", "MM/DD/YYYY", "MMM D, YYYY"] as const;
export const NUMBER_STYLES = ["1,234,567.89", "12,34,567.89", "1.234.567,89", "1 234 567,89"] as const;
export const NEGATIVE_STYLES = ["minus", "parentheses", "red"] as const;

export type PageSize = (typeof PAGE_SIZES)[number];
export type Margins = (typeof MARGINS)[number];
export type FontId = (typeof FONTS)[number];
export type DateFormat = (typeof DATE_FORMATS)[number];
export type NumberStyle = (typeof NUMBER_STYLES)[number];
export type NegativeStyle = (typeof NEGATIVE_STYLES)[number];

/* ------------------------------------------------------------------------------------------
 * Schema
 * ---------------------------------------------------------------------------------------- */

const hex = (fallback: string) =>
  z
    .string()
    .regex(/^#[0-9a-fA-F]{6}$/)
    .catch(fallback);
const text = (fallback: string, max = 200) => z.string().max(max).catch(fallback);
const bool = (fallback: boolean) => z.boolean().catch(fallback);
const oneOf = <const T extends readonly [string, ...string[]]>(values: T, fallback: T[number]) =>
  z.enum(values).catch(fallback);

/** Raster images only; SVG can carry scripts. The backend enforces the same rule. */
const LOGO_PATTERN = /^data:image\/(png|jpeg|webp);base64,[A-Za-z0-9+/=]+$/;
export const MAX_LOGO_BYTES = 1024 * 1024;

const pageSchema = z
  .object({
    size: oneOf(PAGE_SIZES, "A4"),
    orientation: oneOf(["portrait", "landscape"], "portrait"),
    margins: oneOf(MARGINS, "normal"),
  })
  .catch({ size: "A4", orientation: "portrait", margins: "normal" });

const themeDefaults = {
  accent: "#1f3a5f",
  text: "#1c1c1f",
  muted: "#6b6b74",
  font: "inter",
  headingFont: "inter",
  baseSize: 9,
} as const;
const themeSchema = z
  .object({
    accent: hex(themeDefaults.accent),
    text: hex(themeDefaults.text),
    muted: hex(themeDefaults.muted),
    font: oneOf(FONTS, themeDefaults.font),
    headingFont: oneOf(FONTS, themeDefaults.headingFont),
    /** Body size in points. */
    baseSize: z.number().int().min(7).max(13).catch(themeDefaults.baseSize),
  })
  .catch(themeDefaults);

const headerDefaults = {
  visible: true,
  layout: "left",
  style: "rule",
  showLogo: true,
  logo: null,
  logoSize: "medium",
  organization: "Your Company",
  title: "{title}",
  subtitle: "Generated {date}",
  showParameters: true,
} as const;
const headerSchema = z
  .object({
    visible: bool(headerDefaults.visible),
    /** left: logo and title on the left; center: centred; split: title left, logo right. */
    layout: oneOf(["left", "center", "split"], headerDefaults.layout),
    /** plain: no decoration; rule: accent line under the header; band: filled accent band. */
    style: oneOf(["plain", "rule", "band"], headerDefaults.style),
    /** Shows the logo, or a monogram of the organization name when no image is set. */
    showLogo: bool(headerDefaults.showLogo),
    logo: z
      .string()
      .max(Math.ceil((MAX_LOGO_BYTES * 4) / 3) + 64)
      .regex(LOGO_PATTERN)
      .nullable()
      .catch(null),
    logoSize: oneOf(["small", "medium", "large"], headerDefaults.logoSize),
    organization: text(headerDefaults.organization, 120),
    title: text(headerDefaults.title, 160),
    subtitle: text(headerDefaults.subtitle, 200),
    showParameters: bool(headerDefaults.showParameters),
  })
  .catch(headerDefaults);

const tableDefaults = {
  headerStyle: "filled",
  striped: true,
  gridlines: "horizontal",
  density: "normal",
  rowNumbers: false,
  totalsRow: true,
  totalsLabel: "Total",
  uppercaseHeaders: false,
} as const;
const tableSchema = z
  .object({
    /** filled: accent background; tinted: light accent; underline: accent rule; plain: bold text. */
    headerStyle: oneOf(["filled", "tinted", "underline", "plain"], tableDefaults.headerStyle),
    striped: bool(tableDefaults.striped),
    gridlines: oneOf(["none", "horizontal", "all"], tableDefaults.gridlines),
    density: oneOf(["compact", "normal", "relaxed"], tableDefaults.density),
    rowNumbers: bool(tableDefaults.rowNumbers),
    totalsRow: bool(tableDefaults.totalsRow),
    totalsLabel: text(tableDefaults.totalsLabel, 40),
    uppercaseHeaders: bool(tableDefaults.uppercaseHeaders),
  })
  .catch(tableDefaults);

const formatDefaults = {
  date: "DD MMM YYYY",
  numbers: "1,234,567.89",
  decimals: 2,
  negative: "minus",
  currency: "",
} as const;
const formatSchema = z
  .object({
    date: oneOf(DATE_FORMATS, formatDefaults.date),
    numbers: oneOf(NUMBER_STYLES, formatDefaults.numbers),
    decimals: z.number().int().min(0).max(4).catch(formatDefaults.decimals),
    negative: oneOf(NEGATIVE_STYLES, formatDefaults.negative),
    /** Prefixed to money columns, e.g. "₹" or "$". Empty for none. */
    currency: text(formatDefaults.currency, 4),
  })
  .catch(formatDefaults);

const footerDefaults = {
  visible: true,
  style: "rule",
  left: "{organization} · Confidential",
  center: "",
  right: "Page {page} of {pages}",
} as const;
const footerSchema = z
  .object({
    visible: bool(footerDefaults.visible),
    style: oneOf(["plain", "rule"], footerDefaults.style),
    left: text(footerDefaults.left),
    center: text(footerDefaults.center),
    right: text(footerDefaults.right),
  })
  .catch(footerDefaults);

const watermarkDefaults = { text: "", opacity: 8 } as const;
const watermarkSchema = z
  .object({
    text: text(watermarkDefaults.text, 30),
    /** Percent. */
    opacity: z.number().int().min(3).max(25).catch(watermarkDefaults.opacity),
  })
  .catch(watermarkDefaults);

const definitionSchema = z.object({
  schemaVersion: z.literal(SCHEMA_VERSION).catch(SCHEMA_VERSION),
  description: text("", 200),
  page: pageSchema,
  theme: themeSchema,
  header: headerSchema,
  table: tableSchema,
  formats: formatSchema,
  footer: footerSchema,
  watermark: watermarkSchema,
});

export type TemplateDefinition = z.infer<typeof definitionSchema>;

/** Any stored value becomes a complete, valid definition. Never throws. */
export function normalizeDefinition(value: unknown): TemplateDefinition {
  const input = typeof value === "object" && value !== null ? value : {};
  // `.catch` on every field means parsing an object cannot fail.
  return definitionSchema.parse(input);
}

export const DEFAULT_DEFINITION: TemplateDefinition = normalizeDefinition({});

/* ------------------------------------------------------------------------------------------
 * Built-in templates
 * ---------------------------------------------------------------------------------------- */

export interface TemplateEntry {
  /** "builtin:<key>" for built-ins, otherwise the stored UUID. */
  id: string;
  name: string;
  builtIn: boolean;
  definition: TemplateDefinition;
  updatedAt: string | null;
}

type Patch = { [K in keyof TemplateDefinition]?: Partial<TemplateDefinition[K]> | TemplateDefinition[K] };

function preset(key: string, name: string, patch: Patch): TemplateEntry {
  const merged: Record<string, unknown> = { ...DEFAULT_DEFINITION };
  for (const [k, v] of Object.entries(patch)) {
    const base = (DEFAULT_DEFINITION as Record<string, unknown>)[k];
    merged[k] = typeof v === "object" && v !== null && typeof base === "object" ? { ...base, ...v } : v;
  }
  return { id: `builtin:${key}`, name, builtIn: true, definition: normalizeDefinition(merged), updatedAt: null };
}

export const BUILT_IN_TEMPLATES: TemplateEntry[] = [
  preset("corporate", "Corporate", {
    description: "Navy header row and striped rows. A safe choice for most reports.",
    theme: { accent: "#1f3a5f" },
  }),
  preset("modern", "Modern", {
    description: "Bold accent band across the top, roomy rows and soft tinted headings.",
    theme: { accent: "#ea580c", text: "#18181b", muted: "#71717a" },
    header: { style: "band", layout: "split" },
    table: { headerStyle: "tinted", density: "relaxed", gridlines: "none" },
  }),
  preset("minimal", "Minimal", {
    description: "Black and white, thin rules and no fills. Lets the numbers speak.",
    theme: { accent: "#18181b", text: "#18181b", muted: "#71717a" },
    header: { style: "plain", showLogo: false },
    table: { headerStyle: "underline", striped: false, gridlines: "horizontal" },
    footer: { style: "plain" },
  }),
  preset("executive", "Executive", {
    description: "Serif headings, centred title and burgundy accents for board packs.",
    theme: { accent: "#7a1f2b", font: "georgia", headingFont: "georgia", baseSize: 10 },
    header: { layout: "center" },
    table: { headerStyle: "underline", striped: false, density: "relaxed" },
  }),
  preset("ledger", "Finance Ledger", {
    description: "Landscape and compact with full gridlines, row numbers and bracketed negatives.",
    page: { size: "A4", orientation: "landscape", margins: "narrow" },
    theme: { accent: "#0f5132", font: "arial", headingFont: "arial", baseSize: 8 },
    table: { gridlines: "all", density: "compact", rowNumbers: true, striped: false },
    formats: { negative: "parentheses" },
  }),
  preset("print", "Print Friendly", {
    description: "No coloured fills, so it prints cleanly on any office printer and saves ink.",
    theme: { accent: "#3f3f46", text: "#000000", muted: "#52525b" },
    header: { style: "rule" },
    table: { headerStyle: "plain", striped: false, gridlines: "all" },
  }),
];

export function isBuiltIn(id: string) {
  return id.startsWith("builtin:");
}

/** A fresh, unsaved template. */
export function blankTemplate(name = "Untitled Template"): TemplateEntry {
  return { id: "", name, builtIn: false, definition: structuredClone(DEFAULT_DEFINITION), updatedAt: null };
}

/**
 * `base`, or `base 2`, `base 3`... whichever is not taken (case-insensitive).
 *
 * @example uniqueName("Untitled Template", ["Untitled Template"]) // "Untitled Template 2"
 */
export function uniqueName(base: string, taken: string[]): string {
  const names = new Set(taken.map((n) => n.toLowerCase()));
  if (!names.has(base.toLowerCase())) return base;
  for (let i = 2; ; i++) {
    const candidate = `${base} ${i}`;
    if (!names.has(candidate.toLowerCase())) return candidate;
  }
}

/**
 * Name for a copy that doesn't clash with existing names.
 *
 * @example copyName("Corporate", ["Corporate", "Corporate Copy"]) // "Corporate Copy 2"
 */
export function copyName(base: string, taken: string[]): string {
  return uniqueName(`${base.replace(/ Copy( \d+)?$/, "")} Copy`, taken);
}
