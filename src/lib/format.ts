import type { Cell } from "@/services/client";

const countFormat = new Intl.NumberFormat(undefined);
const compactFormat = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });

/** "1,284,310" — uses the system locale's grouping. */
export const formatCount = (n: number) => countFormat.format(n);

/** "1.3M" — for dense places like tree rows. */
export const formatCompact = (n: number) => compactFormat.format(n);

/**
 * Human-readable size in binary units.
 *
 * @example
 * formatBytes(0)          // => "0 B"
 * formatBytes(16_384)     // => "16 KB"
 * formatBytes(412_090_368) // => "393 MB"
 */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value >= 100 ? Math.round(value) : value.toFixed(1).replace(/\.0$/, "")} ${units[unit]}`;
}

/**
 * Inserts locale digit grouping into an exact decimal string without converting it to a
 * float, so values beyond 2^53 or with many decimals stay exact.
 *
 * @example
 * groupDecimal("1234567.50") // => "1,234,567.50" (en-US) or "12,34,567.50" (en-IN)
 * groupDecimal("-0.0001")    // => "-0.0001"
 */
export function groupDecimal(value: string): string {
  const match = /^(-?)(\d+)(\.\d+)?$/.exec(value);
  if (!match) return value;
  const [, sign, whole, fraction = ""] = match;
  return `${sign}${groupDigits(whole ?? "")}${fraction}`;
}

let groupingCache: { sep: string; indian: boolean } | null = null;
function grouping() {
  if (!groupingCache) {
    const parts = new Intl.NumberFormat(undefined).formatToParts(1234567);
    const sep = parts.find((p) => p.type === "group")?.value ?? ",";
    const indian = parts.filter((p) => p.type === "integer").length === 3; // 12,34,567
    groupingCache = { sep, indian };
  }
  return groupingCache;
}

function groupDigits(digits: string): string {
  const { sep, indian } = grouping();
  if (digits.length <= 3) return digits;
  const last3 = digits.slice(-3);
  const rest = digits.slice(0, -3);
  const size = indian ? 2 : 3;
  const groups: string[] = [];
  for (let end = rest.length; end > 0; end -= size) groups.unshift(rest.slice(Math.max(0, end - size), end));
  return `${groups.join(sep)}${sep}${last3}`;
}

export type CellAlign = "left" | "right" | "center";

/** Display text for a grid cell plus how it should be aligned. Never loses precision. */
export function displayCell(cell: Cell): { text: string; align: CellAlign; muted?: boolean; mono?: boolean } {
  switch (cell.t) {
    case "null":
      return { text: "NULL", align: "left", muted: true };
    case "bool":
      return { text: cell.v ? "true" : "false", align: "center", mono: true };
    // Integers are often identifiers (ids, codes, years), so they are shown without grouping.
    case "int":
      return { text: cell.v, align: "right", mono: true };
    case "decimal":
      return { text: groupDecimal(cell.v), align: "right", mono: true };
    case "float":
      return { text: String(cell.v), align: "right", mono: true };
    case "date":
    case "time":
      return { text: cell.v, align: "left", mono: true };
    case "timestamp":
    case "timestamptz":
      return { text: cell.v.replace("T", " "), align: "left", mono: true };
    case "uuid":
      return { text: cell.v, align: "left", mono: true };
    case "json":
      return { text: cell.truncated ? `${cell.v}…` : cell.v, align: "left", mono: true };
    case "text":
      return { text: cell.truncated ? `${cell.v}…` : cell.v, align: "left" };
    case "bytes":
      return { text: `(binary, ${formatBytes(cell.size)})`, align: "left", muted: true };
    case "unsupported":
      return { text: `(${cell.pgType})`, align: "left", muted: true };
  }
}

/** Plain text used when copying a cell: the raw value, not the display formatting. */
export function copyText(cell: Cell): string {
  switch (cell.t) {
    case "null":
      return "";
    case "bool":
      return String(cell.v);
    case "float":
      return String(cell.v);
    case "bytes":
    case "unsupported":
      return "";
    default:
      return cell.v;
  }
}

/**
 * One row as a JSON object. Exact numbers are written as their original digits (never via a
 * float), and json/jsonb values are embedded as JSON rather than as strings.
 *
 * @example
 * rowAsJson([{ name: "id", pgType: "int8" }, { name: "total", pgType: "numeric" }],
 *           [{ t: "int", v: "9007199254740993" }, { t: "decimal", v: "10.50" }])
 * // => '{\n  "id": 9007199254740993,\n  "total": 10.50\n}'
 */
export function rowAsJson(columns: { name: string }[], row: Cell[]): string {
  const fields = columns.map((col, i) => `  ${JSON.stringify(col.name)}: ${cellAsJson(row[i] ?? { t: "null" })}`);
  return `{\n${fields.join(",\n")}\n}`;
}

function cellAsJson(cell: Cell): string {
  switch (cell.t) {
    case "null":
    case "bytes":
    case "unsupported":
      return "null";
    case "bool":
      return String(cell.v);
    case "int":
    case "decimal":
      return cell.v;
    case "float":
      return Number.isFinite(cell.v) ? String(cell.v) : "null";
    case "json":
      return cell.truncated ? JSON.stringify(cell.v) : cell.v;
    default:
      return JSON.stringify(cell.v);
  }
}

/** Tab-separated text that pastes cleanly into Excel. Tabs/newlines inside values become spaces. */
export function asTsv(rows: string[][]): string {
  return rows.map((r) => r.map((v) => v.replace(/[\t\r\n]+/g, " ")).join("\t")).join("\n");
}
