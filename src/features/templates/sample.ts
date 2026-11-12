/**
 * Sample dataset for the template preview. Deterministic, so the preview never jumps around,
 * and shaped like a typical query result: text, dates, integers, money and signed decimals.
 */

export type SampleColumnKind = "text" | "date" | "int" | "money" | "decimal";

export interface SampleColumn {
  key: keyof SampleRow;
  label: string;
  kind: SampleColumnKind;
  /** Relative width; the table scales these to fit the page. */
  width: number;
}

export interface SampleRow {
  invoice: string;
  date: string;
  customer: string;
  region: string;
  units: string;
  amount: string;
  change: string;
}

export const SAMPLE_TITLE = "Monthly Sales Report";
export const SAMPLE_PARAMETERS = [
  { label: "Period", value: "2026-03-01|2026-03-31" },
  { label: "Region", value: "All regions" },
];

export const SAMPLE_COLUMNS: SampleColumn[] = [
  { key: "invoice", label: "Invoice", kind: "text", width: 14 },
  { key: "date", label: "Date", kind: "date", width: 14 },
  { key: "customer", label: "Customer", kind: "text", width: 25 },
  { key: "region", label: "Region", kind: "text", width: 10 },
  { key: "units", label: "Units", kind: "int", width: 8 },
  { key: "amount", label: "Amount", kind: "money", width: 15 },
  { key: "change", label: "Change %", kind: "decimal", width: 14 },
];

const CUSTOMERS = [
  "Northwind Traders",
  "Contoso Pharmaceuticals",
  "Fabrikam Industries",
  "Tailspin Logistics",
  "Wide World Importers",
  "Adventure Works Cycles",
  "Litware Financial",
  "Proseware Retail",
  "Coho Vineyard & Winery",
  "Alpine Ski House",
  "Blue Yonder Airlines",
  "Woodgrove Bank",
  "Lucerne Publishing",
  "Margie's Travel",
];
const REGIONS = ["North", "South", "East", "West", "Central"];

/** Small deterministic generator so every preview shows the same numbers. */
function mulberry32(seed: number) {
  let a = seed;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function buildRows(count: number): SampleRow[] {
  const rand = mulberry32(20260301);
  return Array.from({ length: count }, (_, i) => {
    const units = 1 + Math.floor(rand() * 60);
    const cents = Math.round(units * (12 + rand() * 140) * 100);
    const change = Math.round((rand() * 60 - 18) * 100) / 100;
    const day = 1 + Math.floor((i / count) * 31);
    return {
      invoice: `INV-${24180 + i}`,
      date: `2026-03-${String(day).padStart(2, "0")}`,
      customer: CUSTOMERS[Math.floor(rand() * CUSTOMERS.length)] ?? "",
      region: REGIONS[Math.floor(rand() * REGIONS.length)] ?? "",
      units: String(units),
      amount: (cents / 100).toFixed(2),
      change: change.toFixed(2),
    };
  });
}

export const SAMPLE_ROWS: SampleRow[] = buildRows(128);

/** Exact sum of decimal strings with up to two places, via integer cents. */
function sumCents(values: string[]): bigint {
  return values.reduce((acc, v) => {
    const [i = "0", f = ""] = v.replace("-", "").split(".");
    const cents = BigInt(i) * 100n + BigInt((f + "00").slice(0, 2));
    return v.startsWith("-") ? acc - cents : acc + cents;
  }, 0n);
}

function centsToString(c: bigint): string {
  const neg = c < 0n;
  const abs = neg ? -c : c;
  return `${neg ? "-" : ""}${abs / 100n}.${String(abs % 100n).padStart(2, "0")}`;
}

const amountCents = sumCents(SAMPLE_ROWS.map((r) => r.amount));

export const SAMPLE_TOTALS: Partial<Record<keyof SampleRow, string>> = {
  units: String(SAMPLE_ROWS.reduce((a, r) => a + Number(r.units), 0)),
  amount: centsToString(amountCents),
};
