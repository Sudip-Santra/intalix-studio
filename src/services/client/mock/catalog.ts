/**
 * DEVELOPMENT-ONLY sample database used by the simulated client.
 *
 * A small but realistic sales/reporting schema. Rows are generated deterministically from a
 * seed, so the same table always shows the same data. Nothing here reflects a real database.
 */
import type {
  Cell,
  ColumnInfo,
  ConstraintInfo,
  IndexInfo,
  RelationDetail,
  RelationKind,
  RoutineSummary,
  SchemaSummary,
  SequenceSummary,
} from "../types";

/* ---------------------------------------------------------------------------------------- */
/* Deterministic random                                                                      */
/* ---------------------------------------------------------------------------------------- */

export type Rng = () => number;

/** mulberry32: tiny seeded PRNG so generated rows are stable between renders. */
export function rng(seed: number): Rng {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

const pick = <T>(r: Rng, list: readonly T[]): T => list[Math.floor(r() * list.length)] as T;
const int = (r: Rng, min: number, max: number) => min + Math.floor(r() * (max - min + 1));
const money = (r: Rng, min: number, max: number) => (min + r() * (max - min)).toFixed(2);
const pad = (n: number, w: number) => String(n).padStart(w, "0");

const BASE_DATE = Date.UTC(2026, 0, 1);
const day = (offset: number) => new Date(BASE_DATE + offset * 86_400_000).toISOString().slice(0, 10);
const instant = (r: Rng, maxDays: number) =>
  new Date(BASE_DATE + Math.floor(r() * maxDays * 86_400_000)).toISOString().replace(".000Z", "+00:00");
const uuid = (r: Rng) =>
  "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
    const v = Math.floor(r() * 16);
    return (c === "x" ? v : (v & 0x3) | 0x8).toString(16);
  });

const FIRST = [
  "Aarav",
  "Diya",
  "Kabir",
  "Meera",
  "Rohan",
  "Ananya",
  "Vikram",
  "Isha",
  "Arjun",
  "Sara",
  "Neil",
  "Priya",
];
const LAST = ["Sharma", "Iyer", "Patel", "Reddy", "Gupta", "Nair", "Khan", "Das", "Mehta", "Rao", "Singh", "Bose"];
const COMPANY = [
  "Apex",
  "Northwind",
  "Bluestone",
  "Vertex",
  "Crescent",
  "Summit",
  "Harbor",
  "Lumen",
  "Orion",
  "Keystone",
];
const SUFFIX = ["Industries", "Traders", "Logistics", "Retail", "Foods", "Pharma", "Textiles", "Systems"];
const CITY: [string, string][] = [
  ["Mumbai", "MH"],
  ["Pune", "MH"],
  ["Bengaluru", "KA"],
  ["Chennai", "TN"],
  ["Hyderabad", "TG"],
  ["Kolkata", "WB"],
  ["Ahmedabad", "GJ"],
  ["New Delhi", "DL"],
];
const CATEGORY = ["Hardware", "Software", "Services", "Consumables", "Networking"];
const PRODUCT = [
  "Router",
  "Laptop",
  "License",
  "Toner",
  "Switch",
  "Monitor",
  "Support plan",
  "Cable kit",
  "Server",
  "Scanner",
];

/** Order amounts computed in paise (integers) so subtotal + tax always equals total exactly. */
function orderAmounts(i: number) {
  const subtotal = int(rng(hash(`order-amount-${i}`)), 1_000_00, 4_00_000_00);
  const tax = Math.round((subtotal * 18) / 100);
  const fmt = (paise: number) => `${Math.floor(paise / 100)}.${pad(paise % 100, 2)}`;
  return { subtotal: fmt(subtotal), tax: fmt(tax), total: fmt(subtotal + tax) };
}

/* ---------------------------------------------------------------------------------------- */
/* Catalog definition                                                                        */
/* ---------------------------------------------------------------------------------------- */

type Gen = (row: number, r: Rng) => Cell;

interface MockColumn {
  name: string;
  dataType: string;
  pgType: string;
  nullable?: boolean;
  defaultValue?: string;
  pk?: boolean;
  fk?: { schema: string; table: string; column: string };
  comment?: string;
  gen: Gen;
}

export interface MockRelation {
  schema: string;
  name: string;
  kind: RelationKind;
  owner: string;
  comment: string | null;
  estimatedRows: number;
  sizeBytes: number | null;
  columns: MockColumn[];
  unique?: string[][];
  checks?: { name: string; definition: string; columns: string[] }[];
  extraIndexes?: { name: string; columns: string[]; method?: string }[];
  viewSql?: string;
}

const txt = (v: string): Cell => ({ t: "text", v });
const NULL: Cell = { t: "null" };
const idCol = (comment?: string): MockColumn => ({
  name: "id",
  dataType: "bigint",
  pgType: "int8",
  pk: true,
  defaultValue: "generated always as identity",
  comment,
  gen: (i) => ({ t: "int", v: String(i + 1) }),
});

const RELATIONS: MockRelation[] = [
  {
    schema: "sales",
    name: "customers",
    kind: "table",
    owner: "app_owner",
    comment: "Billing customers. One row per legal entity.",
    estimatedRows: 12_480,
    sizeBytes: 3_276_800,
    unique: [["customer_code"]],
    checks: [
      {
        name: "customers_segment_check",
        columns: ["segment"],
        definition: "CHECK (segment = ANY (ARRAY['Enterprise'::text, 'SMB'::text, 'Retail'::text]))",
      },
    ],
    extraIndexes: [{ name: "customers_city_idx", columns: ["city"] }],
    columns: [
      idCol(),
      {
        name: "customer_code",
        dataType: "text",
        pgType: "text",
        comment: "Stable external code used on invoices.",
        gen: (i) => txt(`CUST-${pad(i + 1001, 6)}`),
      },
      {
        name: "name",
        dataType: "text",
        pgType: "text",
        gen: (_i, r) => txt(`${pick(r, COMPANY)} ${pick(r, SUFFIX)}`),
      },
      {
        name: "email",
        dataType: "text",
        pgType: "text",
        nullable: true,
        gen: (i, r) => (r() < 0.08 ? NULL : txt(`accounts${i + 1}@${pick(r, COMPANY).toLowerCase()}.in`)),
      },
      { name: "city", dataType: "text", pgType: "text", gen: (_i, r) => txt(pick(r, CITY)[0]) },
      { name: "state", dataType: "character(2)", pgType: "bpchar", gen: (_i, r) => txt(pick(r, CITY)[1]) },
      {
        name: "segment",
        dataType: "text",
        pgType: "text",
        gen: (_i, r) => txt(pick(r, ["Enterprise", "SMB", "Retail"] as const)),
      },
      {
        name: "credit_limit",
        dataType: "numeric(14,2)",
        pgType: "numeric",
        nullable: true,
        gen: (_i, r) => (r() < 0.1 ? NULL : { t: "decimal", v: money(r, 50_000, 25_00_000) }),
      },
      {
        name: "is_active",
        dataType: "boolean",
        pgType: "bool",
        defaultValue: "true",
        gen: (_i, r) => ({ t: "bool", v: r() > 0.12 }),
      },
      {
        name: "created_at",
        dataType: "timestamp with time zone",
        pgType: "timestamptz",
        defaultValue: "now()",
        gen: (_i, r) => ({ t: "timestamptz", v: instant(r, 290) }),
      },
    ],
  },
  {
    schema: "sales",
    name: "products",
    kind: "table",
    owner: "app_owner",
    comment: "Sellable catalogue items.",
    estimatedRows: 640,
    sizeBytes: 245_760,
    unique: [["sku"]],
    columns: [
      { ...idCol(), dataType: "integer", pgType: "int4" },
      { name: "sku", dataType: "text", pgType: "text", gen: (i) => txt(`SKU-${pad(i + 1, 5)}`) },
      {
        name: "name",
        dataType: "text",
        pgType: "text",
        gen: (_i, r) => txt(`${pick(r, COMPANY)} ${pick(r, PRODUCT)}`),
      },
      { name: "category", dataType: "text", pgType: "text", gen: (_i, r) => txt(pick(r, CATEGORY)) },
      {
        name: "unit_price",
        dataType: "numeric(12,2)",
        pgType: "numeric",
        gen: (_i, r) => ({ t: "decimal", v: money(r, 199, 2_49_999) }),
      },
      {
        name: "tax_rate",
        dataType: "numeric(5,2)",
        pgType: "numeric",
        defaultValue: "18.00",
        gen: (_i, r) => ({ t: "decimal", v: pick(r, ["5.00", "12.00", "18.00", "28.00"]) }),
      },
      {
        name: "attributes",
        dataType: "jsonb",
        pgType: "jsonb",
        nullable: true,
        gen: (_i, r) =>
          r() < 0.3
            ? NULL
            : { t: "json", v: JSON.stringify({ warranty_months: int(r, 6, 36), origin: pick(r, ["IN", "SG", "DE"]) }) },
      },
      {
        name: "is_active",
        dataType: "boolean",
        pgType: "bool",
        defaultValue: "true",
        gen: (_i, r) => ({ t: "bool", v: r() > 0.05 }),
      },
    ],
  },
  {
    schema: "sales",
    name: "orders",
    kind: "table",
    owner: "app_owner",
    comment: "Customer orders. Amounts are in the order currency.",
    estimatedRows: 1_284_310,
    sizeBytes: 412_090_368,
    unique: [["order_number"]],
    checks: [
      {
        name: "orders_status_check",
        columns: ["status"],
        definition:
          "CHECK (status = ANY (ARRAY['pending'::text, 'confirmed'::text, 'shipped'::text, 'delivered'::text, 'cancelled'::text]))",
      },
      { name: "orders_total_check", columns: ["total_amount"], definition: "CHECK (total_amount >= 0::numeric)" },
    ],
    extraIndexes: [
      { name: "orders_customer_id_idx", columns: ["customer_id"] },
      { name: "orders_order_date_idx", columns: ["order_date"], method: "brin" },
    ],
    columns: [
      idCol(),
      { name: "order_number", dataType: "text", pgType: "text", gen: (i) => txt(`SO-2026-${pad(i + 1, 7)}`) },
      {
        name: "customer_id",
        dataType: "bigint",
        pgType: "int8",
        fk: { schema: "sales", table: "customers", column: "id" },
        gen: (_i, r) => ({ t: "int", v: String(int(r, 1, 12_480)) }),
      },
      { name: "order_date", dataType: "date", pgType: "date", gen: (i) => ({ t: "date", v: day(Math.floor(i / 40)) }) },
      {
        name: "status",
        dataType: "text",
        pgType: "text",
        defaultValue: "'pending'::text",
        gen: (_i, r) => txt(pick(r, ["pending", "confirmed", "shipped", "delivered", "delivered", "cancelled"])),
      },
      {
        name: "currency",
        dataType: "character(3)",
        pgType: "bpchar",
        defaultValue: "'INR'::bpchar",
        gen: (_i, r) => txt(r() < 0.9 ? "INR" : "USD"),
      },
      {
        name: "subtotal",
        dataType: "numeric(14,2)",
        pgType: "numeric",
        gen: (i) => ({ t: "decimal", v: orderAmounts(i).subtotal }),
      },
      {
        name: "tax_amount",
        dataType: "numeric(14,2)",
        pgType: "numeric",
        gen: (i) => ({ t: "decimal", v: orderAmounts(i).tax }),
      },
      {
        name: "total_amount",
        dataType: "numeric(14,2)",
        pgType: "numeric",
        comment: "subtotal + tax_amount, rounded to 2 places.",
        gen: (i) => ({ t: "decimal", v: orderAmounts(i).total }),
      },
      {
        name: "placed_at",
        dataType: "timestamp with time zone",
        pgType: "timestamptz",
        gen: (_i, r) => ({ t: "timestamptz", v: instant(r, 290) }),
      },
      {
        name: "notes",
        dataType: "text",
        pgType: "text",
        nullable: true,
        gen: (_i, r) =>
          r() < 0.85 ? NULL : txt(pick(r, ["Deliver before noon", "Partial shipment OK", "Call on arrival"])),
      },
    ],
  },
  {
    schema: "sales",
    name: "order_items",
    kind: "table",
    owner: "app_owner",
    comment: null,
    estimatedRows: 4_903_220,
    sizeBytes: 905_969_664,
    extraIndexes: [{ name: "order_items_order_id_idx", columns: ["order_id"] }],
    columns: [
      idCol(),
      {
        name: "order_id",
        dataType: "bigint",
        pgType: "int8",
        fk: { schema: "sales", table: "orders", column: "id" },
        gen: (i) => ({ t: "int", v: String(Math.floor(i / 4) + 1) }),
      },
      {
        name: "product_id",
        dataType: "integer",
        pgType: "int4",
        fk: { schema: "sales", table: "products", column: "id" },
        gen: (_i, r) => ({ t: "int", v: String(int(r, 1, 640)) }),
      },
      {
        name: "quantity",
        dataType: "integer",
        pgType: "int4",
        gen: (_i, r) => ({ t: "int", v: String(int(r, 1, 24)) }),
      },
      {
        name: "unit_price",
        dataType: "numeric(12,2)",
        pgType: "numeric",
        gen: (_i, r) => ({ t: "decimal", v: money(r, 199, 99_999) }),
      },
      {
        name: "discount_pct",
        dataType: "numeric(5,2)",
        pgType: "numeric",
        defaultValue: "0",
        gen: (_i, r) => ({ t: "decimal", v: pick(r, ["0.00", "0.00", "5.00", "10.00"]) }),
      },
    ],
  },
  {
    schema: "sales",
    name: "payments",
    kind: "table",
    owner: "app_owner",
    comment: "Payments received against orders.",
    estimatedRows: 1_102_877,
    sizeBytes: 188_743_680,
    columns: [
      {
        name: "id",
        dataType: "uuid",
        pgType: "uuid",
        pk: true,
        defaultValue: "gen_random_uuid()",
        gen: (_i, r) => ({ t: "uuid", v: uuid(r) }),
      },
      {
        name: "order_id",
        dataType: "bigint",
        pgType: "int8",
        fk: { schema: "sales", table: "orders", column: "id" },
        gen: (i) => ({ t: "int", v: String(i + 1) }),
      },
      {
        name: "method",
        dataType: "text",
        pgType: "text",
        gen: (_i, r) => txt(pick(r, ["UPI", "NEFT", "RTGS", "Card", "Cheque"])),
      },
      {
        name: "amount",
        dataType: "numeric(14,2)",
        pgType: "numeric",
        gen: (_i, r) => ({ t: "decimal", v: money(r, 500, 4_50_000) }),
      },
      {
        name: "paid_at",
        dataType: "timestamp with time zone",
        pgType: "timestamptz",
        gen: (_i, r) => ({ t: "timestamptz", v: instant(r, 290) }),
      },
      {
        name: "reference",
        dataType: "text",
        pgType: "text",
        nullable: true,
        gen: (_i, r) => (r() < 0.2 ? NULL : txt(`UTR${int(r, 100000000, 999999999)}`)),
      },
    ],
  },
  {
    schema: "reporting",
    name: "daily_sales",
    kind: "view",
    owner: "report_owner",
    comment: "Delivered and shipped revenue per calendar day.",
    estimatedRows: 290,
    sizeBytes: null,
    viewSql: ` SELECT o.order_date AS sale_date,
    count(*) AS orders,
    sum(o.total_amount) AS revenue,
    round(avg(o.total_amount), 2) AS avg_order_value
   FROM sales.orders o
  WHERE o.status = ANY (ARRAY['shipped'::text, 'delivered'::text])
  GROUP BY o.order_date;`,
    columns: [
      { name: "sale_date", dataType: "date", pgType: "date", gen: (i) => ({ t: "date", v: day(i) }) },
      {
        name: "orders",
        dataType: "bigint",
        pgType: "int8",
        gen: (_i, r) => ({ t: "int", v: String(int(r, 3200, 5400)) }),
      },
      {
        name: "revenue",
        dataType: "numeric",
        pgType: "numeric",
        gen: (_i, r) => ({ t: "decimal", v: money(r, 4_20_00_000, 9_80_00_000) }),
      },
      {
        name: "avg_order_value",
        dataType: "numeric",
        pgType: "numeric",
        gen: (_i, r) => ({ t: "decimal", v: money(r, 12_000, 19_000) }),
      },
    ],
  },
  {
    schema: "reporting",
    name: "customer_lifetime_value",
    kind: "materialized_view",
    owner: "report_owner",
    comment: "Refreshed nightly at 02:00 IST.",
    estimatedRows: 12_480,
    sizeBytes: 1_638_400,
    viewSql: ` SELECT c.id AS customer_id,
    c.name,
    count(o.id) AS total_orders,
    COALESCE(sum(o.total_amount), 0::numeric) AS lifetime_value,
    max(o.order_date) AS last_order_date
   FROM sales.customers c
     LEFT JOIN sales.orders o ON o.customer_id = c.id AND o.status <> 'cancelled'::text
  GROUP BY c.id, c.name;`,
    columns: [
      { name: "customer_id", dataType: "bigint", pgType: "int8", gen: (i) => ({ t: "int", v: String(i + 1) }) },
      { name: "name", dataType: "text", pgType: "text", gen: (_i, r) => txt(`${pick(r, COMPANY)} ${pick(r, SUFFIX)}`) },
      {
        name: "total_orders",
        dataType: "bigint",
        pgType: "int8",
        gen: (_i, r) => ({ t: "int", v: String(int(r, 0, 420)) }),
      },
      {
        name: "lifetime_value",
        dataType: "numeric",
        pgType: "numeric",
        gen: (_i, r) => ({ t: "decimal", v: money(r, 0, 9_50_00_000) }),
      },
      {
        name: "last_order_date",
        dataType: "date",
        pgType: "date",
        nullable: true,
        gen: (_i, r) => (r() < 0.05 ? NULL : { t: "date", v: day(int(r, 0, 289)) }),
      },
    ],
  },
  {
    schema: "reporting",
    name: "monthly_revenue",
    kind: "view",
    owner: "report_owner",
    comment: null,
    estimatedRows: 10,
    sizeBytes: null,
    viewSql: ` SELECT date_trunc('month'::text, sale_date::timestamp with time zone)::date AS month,
    sum(revenue) AS revenue
   FROM reporting.daily_sales
  GROUP BY 1;`,
    columns: [
      { name: "month", dataType: "date", pgType: "date", gen: (i) => ({ t: "date", v: `2026-${pad(i + 1, 2)}-01` }) },
      {
        name: "revenue",
        dataType: "numeric",
        pgType: "numeric",
        gen: (_i, r) => ({ t: "decimal", v: money(r, 1_40_00_00_000, 2_60_00_00_000) }),
      },
    ],
  },
  {
    schema: "public",
    name: "app_settings",
    kind: "table",
    owner: "app_owner",
    comment: null,
    estimatedRows: 18,
    sizeBytes: 16_384,
    columns: [
      { name: "key", dataType: "text", pgType: "text", pk: true, gen: (i) => txt(`feature.flag_${i + 1}`) },
      {
        name: "value",
        dataType: "jsonb",
        pgType: "jsonb",
        gen: (_i, r) => ({ t: "json", v: JSON.stringify({ enabled: r() > 0.5 }) }),
      },
      {
        name: "updated_at",
        dataType: "timestamp with time zone",
        pgType: "timestamptz",
        gen: (_i, r) => ({ t: "timestamptz", v: instant(r, 290) }),
      },
    ],
  },
  {
    schema: "public",
    name: "schema_migrations",
    kind: "table",
    owner: "app_owner",
    comment: null,
    estimatedRows: 42,
    sizeBytes: 16_384,
    columns: [
      {
        name: "version",
        dataType: "bigint",
        pgType: "int8",
        pk: true,
        gen: (i) => ({ t: "int", v: String(20250101000000 + i * 1000) }),
      },
      {
        name: "applied_at",
        dataType: "timestamp with time zone",
        pgType: "timestamptz",
        gen: (_i, r) => ({ t: "timestamptz", v: instant(r, 290) }),
      },
    ],
  },
  {
    schema: "audit",
    name: "events",
    kind: "partitioned_table",
    owner: "audit_owner",
    comment: "Append-only application audit trail, partitioned by month.",
    estimatedRows: 18_772_014,
    sizeBytes: 6_871_947_673,
    extraIndexes: [{ name: "events_occurred_at_idx", columns: ["occurred_at"], method: "brin" }],
    columns: [
      idCol(),
      {
        name: "occurred_at",
        dataType: "timestamp with time zone",
        pgType: "timestamptz",
        gen: (_i, r) => ({ t: "timestamptz", v: instant(r, 290) }),
      },
      {
        name: "actor",
        dataType: "text",
        pgType: "text",
        gen: (_i, r) => txt(`${pick(r, FIRST).toLowerCase()}.${pick(r, LAST).toLowerCase()}`),
      },
      {
        name: "action",
        dataType: "text",
        pgType: "text",
        gen: (_i, r) => txt(pick(r, ["order.create", "order.update", "payment.capture", "customer.update", "login"])),
      },
      {
        name: "entity_id",
        dataType: "text",
        pgType: "text",
        nullable: true,
        gen: (_i, r) => txt(String(int(r, 1, 99999))),
      },
      {
        name: "payload",
        dataType: "jsonb",
        pgType: "jsonb",
        nullable: true,
        gen: (_i, r) => ({
          t: "json",
          v: JSON.stringify({ source: pick(r, ["web", "api", "batch"]), changes: int(r, 1, 6) }),
        }),
      },
      {
        name: "client_ip",
        dataType: "inet",
        pgType: "inet",
        nullable: true,
        gen: (_i, r) => txt(`10.${int(r, 0, 40)}.${int(r, 0, 255)}.${int(r, 1, 254)}`),
      },
    ],
  },
];

const SCHEMAS: SchemaSummary[] = [
  { name: "audit", owner: "audit_owner", comment: "Audit trail. Large, partitioned.", isSystem: false },
  { name: "public", owner: "postgres", comment: "Standard public schema", isSystem: false },
  { name: "reporting", owner: "report_owner", comment: "Curated views for reporting.", isSystem: false },
  { name: "sales", owner: "app_owner", comment: "Order-to-cash data.", isSystem: false },
  { name: "information_schema", owner: "postgres", comment: null, isSystem: true },
  { name: "pg_catalog", owner: "postgres", comment: "System catalog schema", isSystem: true },
];

const ROUTINES: RoutineSummary[] = [
  {
    schema: "reporting",
    name: "revenue_between",
    kind: "function",
    arguments: "p_from date, p_to date",
    returns: "numeric",
  },
  {
    schema: "reporting",
    name: "top_customers",
    kind: "function",
    arguments: "p_limit integer DEFAULT 10",
    returns: "TABLE(customer_id bigint, name text, lifetime_value numeric)",
  },
  { schema: "reporting", name: "refresh_clv", kind: "procedure", arguments: "", returns: null },
];

const SEQUENCES: SequenceSummary[] = [
  { schema: "sales", name: "customers_id_seq", dataType: "bigint", ownedBy: "sales.customers.id" },
  { schema: "sales", name: "orders_id_seq", dataType: "bigint", ownedBy: "sales.orders.id" },
  { schema: "sales", name: "order_items_id_seq", dataType: "bigint", ownedBy: "sales.order_items.id" },
  { schema: "audit", name: "events_id_seq", dataType: "bigint", ownedBy: "audit.events.id" },
];

/* ---------------------------------------------------------------------------------------- */
/* Accessors                                                                                 */
/* ---------------------------------------------------------------------------------------- */

export const mockSchemas = () => SCHEMAS.map((s) => ({ ...s }));

export function mockObjects(schema: string) {
  return {
    relations: RELATIONS.filter((r) => r.schema === schema).map((r) => ({
      schema: r.schema,
      name: r.name,
      kind: r.kind,
      estimatedRows: r.estimatedRows,
      sizeBytes: r.sizeBytes,
      comment: r.comment,
    })),
    routines: ROUTINES.filter((r) => r.schema === schema),
    sequences: SEQUENCES.filter((s) => s.schema === schema),
  };
}

export function findSequence(schema: string | null, name: string): SequenceSummary | undefined {
  const lower = name.toLowerCase();
  return SEQUENCES.find((q) => q.name === lower && (!schema || q.schema === schema.toLowerCase()));
}

export function findRelation(schema: string | null, name: string): MockRelation | undefined {
  const lower = name.toLowerCase();
  if (schema) return RELATIONS.find((r) => r.schema === schema.toLowerCase() && r.name === lower);
  // Mirrors a default search_path of "public" first.
  return RELATIONS.find((r) => r.schema === "public" && r.name === lower) ?? RELATIONS.find((r) => r.name === lower);
}

export function mockCompletionCatalog() {
  const catalog: Record<string, Record<string, string[]>> = {};
  for (const r of RELATIONS) (catalog[r.schema] ??= {})[r.name] = r.columns.map((c) => c.name);
  return catalog;
}

export function relationDetail(rel: MockRelation): RelationDetail {
  const columns: ColumnInfo[] = rel.columns.map((c, i) => ({
    position: i + 1,
    name: c.name,
    dataType: c.dataType,
    nullable: c.nullable ?? false,
    defaultValue: c.defaultValue ?? null,
    isPrimaryKey: c.pk ?? false,
    foreignKey: c.fk ?? null,
    comment: c.comment ?? null,
  }));

  const isView = rel.kind === "view" || rel.kind === "materialized_view";
  const pkCols = columns.filter((c) => c.isPrimaryKey).map((c) => c.name);
  const constraints: ConstraintInfo[] = [];
  const indexes: IndexInfo[] = [];

  if (!isView) {
    if (pkCols.length) {
      constraints.push({
        name: `${rel.name}_pkey`,
        kind: "primary_key",
        columns: pkCols,
        definition: `PRIMARY KEY (${pkCols.join(", ")})`,
        references: null,
      });
      indexes.push(index(rel, `${rel.name}_pkey`, pkCols, { unique: true, primary: true }));
    }
    for (const cols of rel.unique ?? []) {
      const name = `${rel.name}_${cols.join("_")}_key`;
      constraints.push({
        name,
        kind: "unique",
        columns: cols,
        definition: `UNIQUE (${cols.join(", ")})`,
        references: null,
      });
      indexes.push(index(rel, name, cols, { unique: true }));
    }
    for (const c of columns.filter((c) => c.foreignKey)) {
      const fk = c.foreignKey!;
      constraints.push({
        name: `${rel.name}_${c.name}_fkey`,
        kind: "foreign_key",
        columns: [c.name],
        definition: `FOREIGN KEY (${c.name}) REFERENCES ${fk.schema}.${fk.table}(${fk.column})`,
        references: { schema: fk.schema, table: fk.table, columns: [fk.column] },
      });
    }
    for (const ch of rel.checks ?? []) {
      constraints.push({
        name: ch.name,
        kind: "check",
        columns: ch.columns,
        definition: ch.definition,
        references: null,
      });
    }
    for (const ix of rel.extraIndexes ?? []) indexes.push(index(rel, ix.name, ix.columns, { method: ix.method }));
  }

  return {
    summary: {
      schema: rel.schema,
      name: rel.name,
      kind: rel.kind,
      estimatedRows: rel.estimatedRows,
      sizeBytes: rel.sizeBytes,
      comment: rel.comment,
    },
    owner: rel.owner,
    columns,
    indexes,
    constraints,
    ddl: buildDdl(rel, columns, constraints, indexes),
  };
}

function index(
  rel: MockRelation,
  name: string,
  columns: string[],
  opts: { unique?: boolean; primary?: boolean; method?: string },
): IndexInfo {
  const method = opts.method ?? "btree";
  return {
    name,
    columns,
    isUnique: opts.unique ?? false,
    isPrimary: opts.primary ?? false,
    method,
    definition: `CREATE ${opts.unique ? "UNIQUE " : ""}INDEX ${name} ON ${rel.schema}.${rel.name} USING ${method} (${columns.join(", ")})`,
    sizeBytes: rel.sizeBytes ? Math.round(rel.sizeBytes * (opts.primary ? 0.06 : 0.04)) : null,
  };
}

const quoteLiteral = (s: string) => `'${s.replace(/'/g, "''")}'`;

function buildDdl(rel: MockRelation, columns: ColumnInfo[], constraints: ConstraintInfo[], indexes: IndexInfo[]) {
  const qualified = `${rel.schema}.${rel.name}`;
  const comments: string[] = [];
  if (rel.comment) {
    const kindWord = rel.kind === "materialized_view" ? "MATERIALIZED VIEW" : rel.kind === "view" ? "VIEW" : "TABLE";
    comments.push(`COMMENT ON ${kindWord} ${qualified} IS ${quoteLiteral(rel.comment)};`);
  }

  if (rel.kind === "view" || rel.kind === "materialized_view") {
    const head =
      rel.kind === "view" ? `CREATE OR REPLACE VIEW ${qualified} AS` : `CREATE MATERIALIZED VIEW ${qualified} AS`;
    return [`${head}\n${rel.viewSql ?? ""}`, ...comments].join("\n\n") + "\n";
  }

  const width = Math.max(...columns.map((c) => c.name.length));
  const lines = columns.map((c) => {
    const parts = [`    ${c.name.padEnd(width)} ${c.dataType}`];
    if (c.defaultValue?.startsWith("generated")) parts.push(c.defaultValue.toUpperCase());
    else if (c.defaultValue) parts.push(`DEFAULT ${c.defaultValue}`);
    if (!c.nullable) parts.push("NOT NULL");
    return parts.join(" ");
  });
  for (const k of constraints) lines.push(`    CONSTRAINT ${k.name} ${k.definition}`);

  const partition = rel.kind === "partitioned_table" ? " PARTITION BY RANGE (occurred_at)" : "";
  const create = `CREATE TABLE ${qualified} (\n${lines.join(",\n")}\n)${partition};`;
  const extraIdx = indexes
    .filter((i) => !i.isPrimary && !constraints.some((k) => k.name === i.name))
    .map((i) => `${i.definition};`);
  for (const c of columns.filter((c) => c.comment)) {
    comments.push(`COMMENT ON COLUMN ${qualified}.${c.name} IS ${quoteLiteral(c.comment!)};`);
  }
  return [create, extraIdx.join("\n"), comments.join("\n")].filter(Boolean).join("\n\n") + "\n";
}

export function generateRows(rel: MockRelation, count: number, columnNames?: string[]): Cell[][] {
  const cols = columnNames ? rel.columns.filter((c) => columnNames.includes(c.name)) : rel.columns;
  const rows: Cell[][] = [];
  for (let i = 0; i < count; i++) {
    rows.push(cols.map((c) => c.gen(i, rng(hash(`${rel.schema}.${rel.name}.${c.name}.${i}`)))));
  }
  return rows;
}

export const relationColumns = (rel: MockRelation, names?: string[]) =>
  (names ? names.map((n) => rel.columns.find((c) => c.name === n)!) : rel.columns).map((c) => ({
    name: c.name,
    pgType: c.pgType,
  }));
