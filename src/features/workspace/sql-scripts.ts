import type { RelationDetail, SchemaSummary } from "@/services/client";
import { qualified, quoteIdent } from "./workspace-context";

/** ORDER BY clause on the primary key, or "" when the relation has none. */
function pkOrder(detail: RelationDetail | null, direction: "ASC" | "DESC"): string {
  const pk = detail?.columns.filter((c) => c.isPrimaryKey).map((c) => quoteIdent(c.name)) ?? [];
  if (pk.length === 0) return "";
  return `ORDER BY ${pk.map((c) => (direction === "DESC" ? `${c} DESC` : c)).join(", ")}`;
}

const lines = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join("\n") + ";";

/**
 * Builds the "View data" queries the explorer offers, mirroring pgAdmin.
 *
 * @example
 * viewDataSql("sales", "orders", detailWithPk("id"), "first")
 * // => "SELECT *\nFROM sales.orders\nORDER BY id\nLIMIT 100;"
 * viewDataSql("sales", "orders", detailWithPk("id"), "last")
 * // => "SELECT *\nFROM sales.orders\nORDER BY id DESC\nLIMIT 100;"
 * viewDataSql("reporting", "daily_sales", viewDetail, "all")
 * // => "SELECT *\nFROM reporting.daily_sales;"
 */
export function viewDataSql(
  schema: string,
  name: string,
  detail: RelationDetail | null,
  mode: "first" | "last" | "all" | "filtered",
): string {
  const from = `FROM ${qualified(schema, name)}`;
  switch (mode) {
    case "first":
      return lines("SELECT *", from, pkOrder(detail, "ASC"), "LIMIT 100");
    case "last": {
      const order = pkOrder(detail, "DESC");
      return order
        ? lines("SELECT *", from, order, "LIMIT 100")
        : lines(
            '-- This relation has no primary key, so "last" rows are not well defined.',
            "SELECT *",
            from,
            "LIMIT 100",
          );
    }
    case "all":
      return lines("SELECT *", from, pkOrder(detail, "ASC"));
    case "filtered":
      return lines(
        "-- Replace TRUE with your conditions, then press Ctrl+Enter.",
        "SELECT *",
        from,
        "WHERE TRUE",
        pkOrder(detail, "ASC"),
        "LIMIT 100",
      );
  }
}

export const countRowsSql = (schema: string, name: string) => `SELECT count(*)\nFROM ${qualified(schema, name)};`;

/** SELECT with every column listed explicitly, one per line, like pgAdmin's SELECT script. */
export function selectScript(schema: string, name: string, detail: RelationDetail): string {
  const cols = detail.columns.map((c) => `    ${quoteIdent(c.name)}`).join(",\n");
  return `SELECT\n${cols}\nFROM ${qualified(schema, name)}\nLIMIT 100;`;
}

export function createSchemaScript(schema: SchemaSummary): string {
  const parts = [`CREATE SCHEMA ${quoteIdent(schema.name)}\n    AUTHORIZATION ${quoteIdent(schema.owner)};`];
  if (schema.comment)
    parts.push(`COMMENT ON SCHEMA ${quoteIdent(schema.name)} IS '${schema.comment.replace(/'/g, "''")}';`);
  return parts.join("\n\n") + "\n";
}
