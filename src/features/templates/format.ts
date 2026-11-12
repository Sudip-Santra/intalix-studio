/**
 * Number, date and text-token formatting used by report templates.
 *
 * Numbers are formatted from their decimal string, never via floating point, so exact values
 * from PostgreSQL (numeric, bigint, money) are rounded and grouped without precision loss.
 */
import type { DateFormat, NegativeStyle, NumberStyle } from "./model";

const SEPARATORS: Record<NumberStyle, { group: string; decimal: string; indian: boolean }> = {
  "1,234,567.89": { group: ",", decimal: ".", indian: false },
  "12,34,567.89": { group: ",", decimal: ".", indian: true },
  "1.234.567,89": { group: ".", decimal: ",", indian: false },
  "1 234 567,89": { group: " ", decimal: ",", indian: false },
};

/** Adds one to a string of digits, e.g. "199" -> "200". */
function incrementDigits(digits: string): string {
  const out = digits.split("");
  for (let i = out.length - 1; i >= 0; i--) {
    if (out[i] === "9") {
      out[i] = "0";
    } else {
      out[i] = String(Number(out[i]) + 1);
      return out.join("");
    }
  }
  return `1${out.join("")}`;
}

/**
 * Rounds a decimal string half away from zero.
 *
 * @example roundDecimal("12.345", 2) // { int: "12", frac: "35" }
 * @example roundDecimal("9.995", 2)  // { int: "10", frac: "00" }
 */
export function roundDecimal(abs: string, decimals: number): { int: string; frac: string } {
  const [rawInt = "0", rawFrac = ""] = abs.split(".");
  const int = rawInt.replace(/^0+(?=\d)/, "") || "0";
  if (rawFrac.length <= decimals) return { int, frac: rawFrac.padEnd(decimals, "0") };
  const kept = int + rawFrac.slice(0, decimals);
  const roundUp = Number(rawFrac[decimals]) >= 5;
  const rounded = roundUp ? incrementDigits(kept) : kept;
  const intLength = rounded.length - decimals;
  return { int: rounded.slice(0, intLength) || "0", frac: rounded.slice(intLength) };
}

/**
 * Groups the integer digits.
 *
 * @example groupDigits("1234567", ",", false) // "1,234,567"
 * @example groupDigits("1234567", ",", true)  // "12,34,567" (Indian lakh/crore grouping)
 */
export function groupDigits(int: string, separator: string, indian: boolean): string {
  if (int.length <= 3) return int;
  const last3 = int.slice(-3);
  const rest = int.slice(0, -3);
  const size = indian ? 2 : 3;
  const groups: string[] = [];
  for (let end = rest.length; end > 0; end -= size) groups.unshift(rest.slice(Math.max(0, end - size), end));
  return [...groups, last3].join(separator);
}

export interface FormattedNumber {
  text: string;
  negative: boolean;
}

/**
 * Formats an exact decimal string for display.
 *
 * @example formatDecimal("-1234.5", { style: "1,234,567.89", decimals: 2, negative: "parentheses" })
 *          // { text: "(1,234.50)", negative: true }
 * @example formatDecimal("1234567.891", { style: "12,34,567.89", decimals: 2, negative: "minus", currency: "₹" })
 *          // { text: "₹12,34,567.89", negative: false }
 */
export function formatDecimal(
  value: string,
  options: { style: NumberStyle; decimals: number; negative: NegativeStyle; currency?: string },
): FormattedNumber {
  const trimmed = value.trim();
  if (!/^-?\d+(\.\d+)?$/.test(trimmed)) return { text: value, negative: false };
  const isNegative = trimmed.startsWith("-");
  const { int, frac } = roundDecimal(isNegative ? trimmed.slice(1) : trimmed, options.decimals);
  // "-0.00" after rounding is just zero.
  const negative = isNegative && /[1-9]/.test(int + frac);
  const sep = SEPARATORS[options.style];
  const body = groupDigits(int, sep.group, sep.indian) + (frac ? sep.decimal + frac : "");
  const money = `${options.currency ?? ""}${body}`;
  if (!negative) return { text: money, negative: false };
  return { text: options.negative === "parentheses" ? `(${money})` : `-${money}`, negative: true };
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/**
 * Formats an ISO date (YYYY-MM-DD, optionally followed by a time) without time-zone shifts.
 *
 * @example formatDate("2026-03-07", "DD MMM YYYY") // "07 Mar 2026"
 * @example formatDate("2026-03-07", "MMM D, YYYY") // "Mar 7, 2026"
 */
export function formatDate(iso: string, format: DateFormat): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  const [, y = "", mo = "", d = ""] = m;
  const month = MONTHS[Number(mo) - 1] ?? mo;
  switch (format) {
    case "DD MMM YYYY":
      return `${d} ${month} ${y}`;
    case "YYYY-MM-DD":
      return `${y}-${mo}-${d}`;
    case "DD/MM/YYYY":
      return `${d}/${mo}/${y}`;
    case "MM/DD/YYYY":
      return `${mo}/${d}/${y}`;
    case "MMM D, YYYY":
      return `${month} ${Number(d)}, ${y}`;
  }
}

/** Values available to `{token}` placeholders in header and footer text. */
export interface TokenValues {
  title: string;
  organization: string;
  date: string;
  time: string;
  page: number;
  pages: number;
}

export const TOKENS: { token: string; description: string }[] = [
  { token: "{title}", description: "Report name" },
  { token: "{organization}", description: "Organization name" },
  { token: "{date}", description: "Date the report was generated" },
  { token: "{time}", description: "Time the report was generated" },
  { token: "{page}", description: "Current page number" },
  { token: "{pages}", description: "Total number of pages" },
];

/**
 * Replaces known `{token}` placeholders; unknown ones are left as typed.
 *
 * @example fillTokens("Page {page} of {pages}", { ...values, page: 1, pages: 3 }) // "Page 1 of 3"
 */
export function fillTokens(text: string, values: TokenValues): string {
  return text.replace(/\{(title|organization|date|time|page|pages)\}/g, (_, key: keyof TokenValues) =>
    String(values[key]),
  );
}
