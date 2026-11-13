/**
 * DEVELOPMENT-ONLY stand-in for the Rust SQL language service, used in browser previews.
 * It is deliberately simple (regex-based); the real analysis, completion and variable binding
 * live in src-tauri/src/lang and src-tauri/src/pg/variables.rs.
 */
import type {
  QueryOutcome,
  SavedQuery,
  SavedQueryDraft,
  SqlAnalysis,
  SqlCheckOutcome,
  SqlCompletionItem,
  SqlCompletions,
  SqlDiagnostic,
  VariableDef,
  VariableOptionsOutcome,
  VariableValue,
} from "../types";
import { mockCompletionCatalog } from "./catalog";
import { simulateQuery, wait } from "./query";

const KEYWORDS = [
  "SELECT",
  "FROM",
  "WHERE",
  "GROUP BY",
  "ORDER BY",
  "HAVING",
  "LIMIT",
  "JOIN",
  "LEFT JOIN",
  "ON",
  "AND",
  "OR",
  "NOT",
  "IN",
  "IS NULL",
  "IS NOT NULL",
  "BETWEEN",
  "LIKE",
  "ILIKE",
  "AS",
  "CASE",
  "WHEN",
  "THEN",
  "ELSE",
  "END",
  "DISTINCT",
  "UNION ALL",
  "WITH",
  "ASC",
  "DESC",
];
const FUNCTIONS = ["count", "sum", "avg", "min", "max", "coalesce", "date_trunc", "to_char", "round", "lower", "upper"];

/** Comments and string literals replaced by spaces, so positions stay the same. */
function blank(sql: string): string {
  return sql
    .replace(/--[^\n]*/g, (m) => " ".repeat(m.length))
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "))
    .replace(/'(?:[^']|'')*'/g, (m) => " ".repeat(m.length));
}

function references(sql: string) {
  const out: { name: string; part: string | null; from: number; to: number }[] = [];
  const re = /\{\{\s*([A-Za-z_]\w*)(?:\.([A-Za-z_]\w*))?\s*\}\}/g;
  for (const m of blank(sql).matchAll(re)) {
    out.push({
      name: (m[1] ?? "").toLowerCase(),
      part: m[2]?.toLowerCase() ?? null,
      from: m.index,
      to: m.index + m[0].length,
    });
  }
  return out;
}

export function mockAnalyze(sql: string, variables: VariableDef[] | null): SqlAnalysis {
  const diagnostics: SqlDiagnostic[] = [];
  const refs = references(sql);
  for (const r of refs) {
    const def = variables?.find((v) => v.name === r.name);
    let message: string | null = null;
    if (!variables) message = "Variables work in the SQL Editor page.";
    else if (!def) message = `{{${r.name}}} is not defined.`;
    else if (def.type === "date_range" && r.part !== "from" && r.part !== "to")
      message = `${r.name} is a date range. Use {{${r.name}.from}} and {{${r.name}.to}}.`;
    else if (def.type !== "date_range" && r.part) message = `Only date ranges have .from and .to.`;
    if (message) diagnostics.push({ from: r.from, to: r.to, severity: "error", message, hint: null });
  }
  const text = blank(sql);
  const stack: number[] = [];
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "(") stack.push(i);
    if (text[i] === ")") {
      if (stack.length) stack.pop();
      else
        diagnostics.push({
          from: i,
          to: i + 1,
          severity: "error",
          message: "This ) has no matching opening bracket.",
          hint: null,
        });
    }
  }
  for (const i of stack)
    diagnostics.push({ from: i, to: i + 1, severity: "error", message: "This ( is never closed.", hint: null });
  const statements = text.split(";").filter((s) => s.trim()).length;
  return { diagnostics: diagnostics.sort((a, b) => a.from - b.from), references: refs, statements };
}

export function mockComplete(sql: string, cursor: number, variables: VariableDef[] | null): SqlCompletions {
  let from = cursor;
  while (from > 0 && /[\w$]/.test(sql[from - 1] ?? "")) from--;
  const before = sql.slice(0, from);
  const prefix = sql.slice(from, cursor).toLowerCase();
  const item = (
    label: string,
    kind: SqlCompletionItem["kind"],
    boost: number,
    extra: Partial<SqlCompletionItem> = {},
  ) =>
    ({
      label,
      kind,
      detail: null,
      info: null,
      apply: null,
      snippet: false,
      boost,
      ...extra,
    }) satisfies SqlCompletionItem;

  let items: SqlCompletionItem[] = [];
  if (/\{\{\s*$/.test(before)) {
    if (!variables) return { from, items: [] };
    for (const v of variables) {
      const labels = v.type === "date_range" ? [`${v.name}.from`, `${v.name}.to`] : [v.name];
      for (const l of labels) items.push(item(l, "variable", 5, { detail: v.type, apply: `${l}}}` }));
    }
  } else {
    const catalog = mockCompletionCatalog();
    const afterFrom = /\b(from|join)\s+$/i.test(before);
    for (const [schema, rels] of Object.entries(catalog)) {
      for (const [rel, cols] of Object.entries(rels)) {
        const label = schema === "public" ? rel : `${schema}.${rel}`;
        items.push(item(label, "table", afterFrom ? 5 : 1, { detail: `table · ${schema}` }));
        if (!afterFrom && new RegExp(`\\b${rel}\\b`, "i").test(sql))
          for (const c of cols) items.push(item(c, "column", 4, { detail: rel }));
      }
    }
    if (!afterFrom) {
      for (const f of FUNCTIONS) items.push(item(f, "function", 1, { apply: `${f}(\${})`, snippet: true }));
      for (const k of KEYWORDS) items.push(item(k, "keyword", 0));
      for (const v of variables ?? [])
        items.push(item(v.name, "variable", 1, { detail: `{{${v.name}}}`, apply: `{{${v.name}}}` }));
    }
  }
  if (prefix) items = items.filter((i) => i.label.toLowerCase().includes(prefix));
  return { from, items: items.sort((a, b) => b.boost - a.boost).slice(0, 300) };
}

const PG_TYPES: Record<VariableDef["type"], string> = {
  text: "text",
  number: "numeric",
  date: "date",
  date_range: "date",
  enum: "text",
  multi_enum: "text[]",
};

export async function mockCheck(sql: string, variables: VariableDef[], signal?: AbortSignal): Promise<SqlCheckOutcome> {
  await wait(250, signal);
  const analysis = mockAnalyze(sql, variables);
  const error = analysis.diagnostics.find((d) => d.severity === "error");
  if (error)
    return {
      ok: false,
      category: "syntax_error",
      message: error.message,
      hint: null,
      position: [...sql.slice(0, error.from)].length + 1,
      durationMs: 3,
      simulated: true,
    };
  const columns = /\bfrom\b/i.test(sql)
    ? [
        { name: "id", pgType: "int8" },
        { name: "name", pgType: "text" },
      ]
    : [];
  const parameters = [...new Set(analysis.references.map((r) => (r.part ? `${r.name}.${r.part}` : r.name)))].map(
    (reference) => ({
      reference,
      pgType: PG_TYPES[variables.find((v) => v.name === reference.split(".")[0])?.type ?? "text"],
    }),
  );
  return { ok: true, columns, parameters, durationMs: 4 };
}

function isEmpty(value: VariableValue | undefined): boolean {
  if (value == null) return true;
  if (typeof value === "string") return value.trim() === "";
  if (Array.isArray(value)) return value.length === 0;
  return !value.from && !value.to;
}

export async function mockRun(
  sql: string,
  variables: VariableDef[],
  values: Record<string, VariableValue>,
  rowLimit: number,
  signal?: AbortSignal,
): Promise<QueryOutcome> {
  for (const v of variables) {
    if (v.required && isEmpty(values[v.name]))
      return {
        ok: false,
        category: "invalid_input",
        message: `${v.label || v.name}: enter a value. This filter is required.`,
        hint: "Fix the filter value and run again.",
        position: null,
        durationMs: 0,
        simulated: true,
      };
  }
  // The simulator only needs SQL it can read; values are irrelevant to its fake rows.
  const plain = sql.replace(/\{\{[^}]*\}\}/g, "NULL");
  return simulateQuery(plain, rowLimit, signal);
}

export async function mockVariableOptions(sql: string, signal?: AbortSignal): Promise<VariableOptionsOutcome> {
  await wait(300, signal);
  if (!sql.trim())
    return {
      ok: false,
      category: "syntax_error",
      message: "There is no SQL to run.",
      hint: null,
      position: null,
      durationMs: 0,
      simulated: true,
    };
  const values = ["North", "South", "East", "West", "Central"];
  return { ok: true, options: values.map((v) => ({ value: v.toLowerCase(), label: v })), truncated: false };
}

/* ------------------------------------------------------------------------------------------
 * Saved queries, kept in localStorage so a preview survives a reload
 * ---------------------------------------------------------------------------------------- */

const QUERIES_KEY = "intalix.simulated.queries";

export function loadMockQueries(): SavedQuery[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(QUERIES_KEY) ?? "[]");
    return Array.isArray(parsed) ? (parsed as SavedQuery[]) : [];
  } catch {
    return [];
  }
}

export function saveMockQuery(draft: SavedQueryDraft, id?: string): SavedQuery {
  const list = loadMockQueries();
  const now = new Date().toISOString();
  const existing = id ? list.find((q) => q.id === id) : undefined;
  if (id && !existing) throw new Error("This query no longer exists.");
  if (!draft.name.trim()) throw new Error("Give the query a name.");
  const saved: SavedQuery = {
    ...structuredClone(draft),
    name: draft.name.trim(),
    id: existing?.id ?? crypto.randomUUID(),
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
  };
  storeMockQueries([saved, ...list.filter((q) => q.id !== saved.id)]);
  return structuredClone(saved);
}

export function removeMockQuery(id: string) {
  storeMockQueries(loadMockQueries().filter((q) => q.id !== id));
}

function storeMockQueries(list: SavedQuery[]) {
  try {
    localStorage.setItem(QUERIES_KEY, JSON.stringify(list));
  } catch {
    // Storage may be unavailable or full; the preview simply won't persist.
  }
}
