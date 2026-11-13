/**
 * SQL Editor model: drafts, variable definitions and filter values.
 *
 * Variables are written in the SQL as `{{name}}` (date ranges as `{{name.from}}` and
 * `{{name.to}}`). A reference with no definition gets one automatically, with a type guessed
 * from its name; the author then adjusts it in the Variables panel. Values are never pasted
 * into the SQL: the backend binds them as PostgreSQL parameters.
 */
import type {
  DateRangeValue,
  SavedQuery,
  SqlVariableReference,
  VariableDef,
  VariableType,
  VariableValue,
} from "@/services/client";

export interface QueryDraft {
  /** Empty for a query that has never been saved. */
  id: string;
  name: string;
  description: string;
  sql: string;
  variables: VariableDef[];
}

export const TYPE_LABELS: Record<VariableType, string> = {
  text: "Text",
  number: "Number",
  date: "Date",
  date_range: "Date range",
  enum: "Enum",
  multi_enum: "Multi-select",
};

export const NEW_QUERY_SQL = `-- Add filters as {{variables}}, for example:
--   WHERE order_date BETWEEN {{period.from}} AND {{period.to}}
SELECT
`;

export function blankDraft(name: string): QueryDraft {
  return { id: "", name, description: "", sql: NEW_QUERY_SQL, variables: [] };
}

export const toDraft = (q: SavedQuery): QueryDraft => ({
  id: q.id,
  name: q.name,
  description: q.description,
  sql: q.sql,
  variables: structuredClone(q.variables),
});

export const sameDraft = (a: QueryDraft, b: QueryDraft) =>
  a.name === b.name &&
  a.description === b.description &&
  a.sql === b.sql &&
  JSON.stringify(a.variables) === JSON.stringify(b.variables);

/** "start_date" → "Start date" */
export function labelFromName(name: string): string {
  const words = name.replace(/_+/g, " ").trim();
  return words ? words[0]!.toUpperCase() + words.slice(1) : name;
}

/** A type that fits how the variable is named and used. */
function guessType(name: string, parts: (string | null)[], inList: boolean): VariableType {
  if (parts.some((p) => p === "from" || p === "to")) return "date_range";
  if (inList) return "multi_enum";
  if (/(^|_)(date|day|from|to|since|until|start|end)($|_)/.test(name)) return "date";
  if (/(^|_)(amount|min|max|qty|quantity|count|limit|price|total|number|num)($|_)/.test(name)) return "number";
  return "text";
}

export function newVariable(name: string, type: VariableType = "text"): VariableDef {
  return {
    name,
    label: labelFromName(name),
    type,
    required: true,
    default: null,
    // Multi-select needs no setup: its choices are the values of the column it filters.
    options: type === "enum" ? { source: "list", values: [] } : null,
    help: "",
  };
}

/** Same rule as the backend: lower-case letters, digits and _, not starting with a digit. */
export const isValidName = (name: string) => /^[a-z_][a-z0-9_]{0,62}$/.test(name);

/**
 * Brings the definitions in line with the references found in the SQL.
 *
 * - Referenced names without a definition get a new one.
 * - Definitions that were created automatically and never edited are removed again when the
 *   SQL no longer uses them (so typing `{{reg` … `{{region}}` doesn't leave junk behind).
 *
 * Returns `null` when nothing changes.
 */
export function syncVariables(
  defs: VariableDef[],
  references: SqlVariableReference[],
  untouched: Set<string>,
  sql = "",
): VariableDef[] | null {
  const used = new Map<string, (string | null)[]>();
  const inList = new Set<string>();
  for (const r of references) {
    used.set(r.name, [...(used.get(r.name) ?? []), r.part]);
    // `IN ({{x}})` and `= ANY({{x}})` take a list.
    if (/\b(any|in)\s*\(\s*$/i.test(sql.slice(Math.max(0, r.from - 20), r.from))) inList.add(r.name);
  }

  let changed = false;
  const next = defs.filter((d) => {
    const drop = untouched.has(d.name) && !used.has(d.name);
    if (drop) {
      untouched.delete(d.name);
      changed = true;
    }
    return !drop;
  });
  for (const [name, parts] of used) {
    if (!isValidName(name) || next.some((d) => d.name === name)) continue;
    next.push(newVariable(name, guessType(name, parts, inList.has(name))));
    untouched.add(name);
    changed = true;
  }
  return changed ? next : null;
}

/** Changing the type keeps label, required and help, and resets what no longer fits. */
export function changeType(def: VariableDef, type: VariableType): VariableDef {
  return {
    ...def,
    type,
    default: null,
    // Enum keeps its list; Multi-select reads the values of the column it filters instead.
    options: type === "enum" ? (def.options ?? { source: "list", values: [] }) : null,
  };
}

export function isEmptyValue(value: VariableValue | undefined): boolean {
  if (value == null) return true;
  if (typeof value === "string") return value.trim() === "";
  if (Array.isArray(value)) return value.length === 0;
  return !value.from && !value.to;
}

/** Problem with a value as typed, before it is sent (the backend checks again). */
export function valueProblem(def: VariableDef, value: VariableValue | undefined): string | null {
  if (isEmptyValue(value)) return def.required ? "Required" : null;
  if (
    def.type === "number" &&
    typeof value === "string" &&
    !/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d{1,4})?$/.test(value.trim())
  )
    return "Not a number";
  if (def.type === "date_range" && value && typeof value === "object" && !Array.isArray(value)) {
    const r = value as DateRangeValue;
    if (!r.from || !r.to) return "Choose both dates";
    if (r.from > r.to) return "Start is after end";
  }
  return null;
}

/* ------------------------------------------------------------------------------------------
 * Date helpers (local calendar dates as YYYY-MM-DD)
 * ---------------------------------------------------------------------------------------- */

export function isoDate(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Quick ranges for date range filters, computed from today. */
export function rangePresets(today = new Date()): { label: string; value: DateRangeValue }[] {
  const y = today.getFullYear();
  const m = today.getMonth();
  const day = (offset: number) => new Date(y, m, today.getDate() + offset);
  const quarter = Math.floor(m / 3) * 3;
  return [
    { label: "Today", value: { from: isoDate(today), to: isoDate(today) } },
    { label: "Last 7 days", value: { from: isoDate(day(-6)), to: isoDate(today) } },
    { label: "Last 30 days", value: { from: isoDate(day(-29)), to: isoDate(today) } },
    { label: "This month", value: { from: isoDate(new Date(y, m, 1)), to: isoDate(new Date(y, m + 1, 0)) } },
    { label: "Last month", value: { from: isoDate(new Date(y, m - 1, 1)), to: isoDate(new Date(y, m, 0)) } },
    {
      label: "This quarter",
      value: { from: isoDate(new Date(y, quarter, 1)), to: isoDate(new Date(y, quarter + 3, 0)) },
    },
    { label: "This year", value: { from: `${y}-01-01`, to: `${y}-12-31` } },
    { label: "Last year", value: { from: `${y - 1}-01-01`, to: `${y - 1}-12-31` } },
  ];
}

/** Choices as editable text: one per line, `value | Label` when a label is set. */
export function choicesToText(values: { value: string; label: string }[]): string {
  return values.map((o) => (o.label && o.label !== o.value ? `${o.value} | ${o.label}` : o.value)).join("\n");
}

export function textToChoices(text: string): { value: string; label: string }[] {
  const seen = new Set<string>();
  const out: { value: string; label: string }[] = [];
  for (const line of text.split("\n")) {
    const [rawValue, ...rest] = line.split("|");
    const value = (rawValue ?? "").trim();
    if (!value || seen.has(value)) continue;
    seen.add(value);
    out.push({ value, label: rest.join("|").trim() });
  }
  return out;
}

export function uniqueName(base: string, taken: string[]): string {
  const lower = new Set(taken.map((t) => t.toLowerCase()));
  if (!lower.has(base.toLowerCase())) return base;
  for (let i = 2; ; i++) {
    const candidate = `${base} ${i}`;
    if (!lower.has(candidate.toLowerCase())) return candidate;
  }
}
