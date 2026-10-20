/**
 * DEVELOPMENT-ONLY query simulator. It understands just enough SQL to make the workspace
 * believable: `SELECT cols|* FROM [schema.]table [LIMIT n]`, `SELECT count(*) FROM ...`,
 * and literal selects such as `SELECT 1`. Anything else returns the table's sample rows.
 *
 * The read-only policy here mirrors what the Rust backend will enforce (one statement,
 * read-only statement types), so the UI's error states can be built against it now.
 * `pg_sleep` makes a query run for a few seconds so cancellation can be tried.
 */
import type { Cell, QueryErrorCategory, QueryOutcome } from "../types";
import { findRelation, findSequence, generateRows, relationColumns } from "./catalog";

const READ_ONLY_KEYWORDS = new Set(["select", "with", "values", "table", "show", "explain"]);

/** Removes comments and string literals so keyword checks are not fooled by their contents. */
function stripForAnalysis(sql: string): string {
  return sql
    .replace(/--[^\n]*/g, " ")
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/'(?:[^']|'')*'/g, "''")
    .replace(/"(?:[^"]|"")*"/g, '""');
}

function failure(
  category: QueryErrorCategory,
  message: string,
  hint: string | null,
  position: number | null,
  durationMs: number,
): QueryOutcome {
  return { ok: false, category, message, hint, position, durationMs, simulated: true };
}

export function wait(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(new DOMException("Aborted", "AbortError"));
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        reject(new DOMException("Aborted", "AbortError"));
      },
      { once: true },
    );
  });
}

export async function simulateQuery(sql: string, rowLimit: number, signal?: AbortSignal): Promise<QueryOutcome> {
  const started = performance.now();
  const elapsed = () => Math.round(performance.now() - started);
  const clean = stripForAnalysis(sql).trim().replace(/;\s*$/, "");

  if (!clean) return failure("syntax_error", "There is no SQL to run.", "Type a query first.", null, 0);

  if (clean.includes(";")) {
    return failure(
      "query_rejected",
      "Only one statement can run at a time.",
      "Remove the extra statements, or select the one you want and run just that.",
      clean.indexOf(";") + 1,
      0,
    );
  }

  const keyword = /^\(*\s*([a-z]+)/i.exec(clean)?.[1]?.toLowerCase() ?? "";
  if (!READ_ONLY_KEYWORDS.has(keyword)) {
    return failure(
      "query_rejected",
      `${keyword.toUpperCase() || "This"} statements are not allowed. Intalix runs queries read-only.`,
      "Use SELECT, WITH, VALUES, TABLE, SHOW or EXPLAIN. To change data, use your database administration tool.",
      1,
      0,
    );
  }
  if (keyword === "with" && /\b(insert|update|delete|merge)\b/i.test(clean)) {
    return failure(
      "query_rejected",
      "Data-modifying statements inside WITH are not allowed. Intalix runs queries read-only.",
      null,
      1,
      0,
    );
  }

  const sleep = /pg_sleep\s*\(\s*(\d+(?:\.\d+)?)\s*\)/i.exec(clean);
  await wait(sleep ? Math.min(Number(sleep[1]) * 1000, 8000) : 120 + Math.random() * 380, signal);

  const from = /\bfrom\s+(?:([a-z_][a-z0-9_]*)\.)?([a-z_][a-z0-9_]*)/i.exec(clean);
  if (!from) {
    // Keep string literals intact here so SELECT 'text' shows the text.
    const withLiterals = sql
      .replace(/--[^\n]*/g, " ")
      .replace(/\/\*[\s\S]*?\*\//g, " ")
      .trim()
      .replace(/;\s*$/, "");
    return literalSelect(withLiterals, elapsed());
  }

  const [, schema, name] = from;
  const rel = findRelation(schema ?? null, name ?? "");
  const sequence = rel ? undefined : findSequence(schema ?? null, name ?? "");
  if (sequence) {
    return {
      ok: true,
      durationMs: elapsed(),
      simulated: true,
      result: {
        columns: [
          { name: "last_value", pgType: "int8" },
          { name: "is_called", pgType: "bool" },
        ],
        rows: [
          [
            { t: "int", v: String(1000 + ((sequence.name.length * 7919) % 90000)) },
            { t: "bool", v: true },
          ],
        ],
        truncated: false,
        rowLimit,
      },
    };
  }
  if (!rel) {
    const shown = schema ? `${schema}.${name}` : name;
    return failure(
      "undefined_object",
      `relation "${shown}" does not exist`,
      "Check the spelling, or qualify the name with its schema (for example sales.orders).",
      (from.index ?? 0) + from[0].indexOf(name ?? "") + 1,
      elapsed(),
    );
  }

  const limitMatch = /\blimit\s+(\d+)/i.exec(clean);
  const requested = limitMatch ? Number(limitMatch[1]) : Infinity;
  const available = Math.min(requested, rel.estimatedRows);
  const count = Math.min(available, rowLimit);

  if (/^select\s+count\s*\(\s*\*\s*\)\s+from/i.test(clean)) {
    return {
      ok: true,
      durationMs: elapsed(),
      simulated: true,
      result: {
        columns: [{ name: "count", pgType: "int8" }],
        rows: [[{ t: "int", v: String(rel.estimatedRows) }]],
        truncated: false,
        rowLimit,
      },
    };
  }

  const list = /^select\s+(?:distinct\s+)?([\s\S]+?)\s+from\s/i.exec(clean)?.[1] ?? "*";
  const names = list.split(",").map((s) =>
    s
      .trim()
      .replace(/^[a-z_][a-z0-9_]*\./i, "")
      .toLowerCase(),
  );
  const known = new Set(rel.columns.map((c) => c.name));
  const projected = list.trim() !== "*" && names.every((n) => known.has(n)) ? names : undefined;

  return {
    ok: true,
    durationMs: elapsed(),
    simulated: true,
    result: {
      columns: relationColumns(rel, projected),
      rows: generateRows(rel, count, projected),
      truncated: available > rowLimit,
      rowLimit,
    },
  };
}

function literalSelect(clean: string, durationMs: number): QueryOutcome {
  const list = /^select\s+([\s\S]+)$/i.exec(clean)?.[1];
  if (!list) {
    return failure("syntax_error", "The simulated backend can only run SELECT queries.", null, null, durationMs);
  }
  const items = list.split(",").map((s) => s.trim());
  const columns = items.map((item, i) => {
    const alias = /\bas\s+([a-z_][a-z0-9_]*)$/i.exec(item)?.[1];
    return {
      name: alias ?? (items.length === 1 ? "?column?" : `column${i + 1}`),
      expr: item.replace(/\s+as\s+\S+$/i, ""),
    };
  });
  const row: Cell[] = columns.map(({ expr }) => {
    if (/^-?\d+$/.test(expr)) return { t: "int", v: expr };
    if (/^-?\d+\.\d+$/.test(expr)) return { t: "decimal", v: expr };
    if (/^'.*'$/.test(expr)) return { t: "text", v: expr.slice(1, -1) };
    if (/^(true|false)$/i.test(expr)) return { t: "bool", v: expr.toLowerCase() === "true" };
    if (/^now\(\)$|^current_timestamp$/i.test(expr)) return { t: "timestamptz", v: new Date().toISOString() };
    if (/^null$/i.test(expr)) return { t: "null" };
    return { t: "text", v: expr };
  });
  const pgType = (c: Cell) =>
    ({ int: "int4", decimal: "numeric", bool: "bool", timestamptz: "timestamptz" })[c.t as string] ?? "text";
  return {
    ok: true,
    durationMs,
    simulated: true,
    result: {
      columns: columns.map((c, i) => ({ name: c.name, pgType: pgType(row[i]!) })),
      rows: [row],
      truncated: false,
      rowLimit: 1,
    },
  };
}
