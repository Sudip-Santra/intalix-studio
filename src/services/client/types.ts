/**
 * Contract between the UI and the backend.
 *
 * The UI only ever talks to an `IntalixClient`. Today that is the simulated client in
 * `mock-client.ts`; when the Rust/Tauri backend lands, a Tauri-backed client implements
 * the same interface and the screens do not change.
 *
 * Security rule: a password may travel UI -> backend inside a `ConnectionDraft`, but the
 * backend never sends one back. `ConnectionProfile` only says whether a password is stored.
 */

export const ENVIRONMENTS = ["development", "staging", "production"] as const;
export type Environment = (typeof ENVIRONMENTS)[number];

export const SSL_MODES = ["disable", "prefer", "require", "verify-ca", "verify-full"] as const;
export type SslMode = (typeof SSL_MODES)[number];

export interface ConnectionDraft {
  name: string;
  environment: Environment;
  host: string;
  port: number;
  database: string;
  username: string;
  /** Empty when editing a profile whose stored password should be kept. */
  password: string;
  savePassword: boolean;
  sslMode: SslMode;
  caCertPath: string;
  connectTimeoutSecs: number;
  queryTimeoutSecs: number;
  maxConnections: number;
  applicationName: string;
}

export interface ConnectionProfile {
  id: string;
  name: string;
  environment: Environment;
  host: string;
  port: number;
  database: string;
  username: string;
  sslMode: SslMode;
  caCertPath: string;
  connectTimeoutSecs: number;
  queryTimeoutSecs: number;
  maxConnections: number;
  applicationName: string;
  hasStoredPassword: boolean;
  createdAt: string;
  lastConnectedAt: string | null;
}

/** Every error category the backend can report (mirrors `ErrorCategory` in src-tauri/src/error.rs). */
export type ErrorCategory =
  | "connection_failed"
  | "authentication_failed"
  | "database_not_found"
  | "tls_error"
  | "timeout"
  | "query_rejected"
  | "syntax_error"
  | "undefined_object"
  | "permission_denied"
  | "query_failed"
  | "connection_lost"
  | "cancelled"
  | "not_found"
  | "invalid_input"
  | "credential_error"
  | "storage_error"
  | "internal";

export type ConnectionErrorCategory = ErrorCategory;

export type ConnectionTestResult =
  | {
      ok: true;
      serverVersion: string;
      latencyMs: number;
      /** Negotiated TLS version, or null when the connection is not encrypted. */
      tls: string | null;
      simulated: boolean;
    }
  | {
      ok: false;
      category: ConnectionErrorCategory;
      message: string;
      hint: string | null;
      simulated: boolean;
    };

export interface SaveConnectionResult {
  test: ConnectionTestResult;
  /** Present only when the test succeeded and the profile was saved. */
  profile: ConnectionProfile | null;
}

/* ------------------------------------------------------------------------------------------
 * Schema metadata
 * ---------------------------------------------------------------------------------------- */

export interface DatabaseInfo {
  name: string;
  serverVersion: string;
  sizeBytes: number | null;
  /** The database role Intalix is connected as. */
  currentUser: string;
}

export interface SchemaSummary {
  name: string;
  owner: string;
  comment: string | null;
  /** True for pg_catalog / information_schema. Hidden by default in the explorer. */
  isSystem: boolean;
}

export type RelationKind = "table" | "partitioned_table" | "view" | "materialized_view" | "foreign_table";

export interface RelationSummary {
  schema: string;
  name: string;
  kind: RelationKind;
  /** Planner estimate from pg_class.reltuples; null when never analysed. Not an exact count. */
  estimatedRows: number | null;
  sizeBytes: number | null;
  comment: string | null;
}

export interface RoutineSummary {
  schema: string;
  name: string;
  kind: "function" | "procedure" | "aggregate";
  /** Argument list as PostgreSQL prints it, e.g. "p_from date, p_to date". */
  arguments: string;
  returns: string | null;
}

export interface SequenceSummary {
  schema: string;
  name: string;
  dataType: string;
  ownedBy: string | null;
}

export interface SchemaObjects {
  relations: RelationSummary[];
  routines: RoutineSummary[];
  sequences: SequenceSummary[];
}

export interface ForeignKeyRef {
  schema: string;
  table: string;
  column: string;
}

export interface ColumnInfo {
  position: number;
  name: string;
  /** Formatted type as PostgreSQL prints it, e.g. "numeric(12,2)", "timestamp with time zone". */
  dataType: string;
  nullable: boolean;
  defaultValue: string | null;
  isPrimaryKey: boolean;
  foreignKey: ForeignKeyRef | null;
  comment: string | null;
}

export interface IndexInfo {
  name: string;
  columns: string[];
  isUnique: boolean;
  isPrimary: boolean;
  method: string;
  definition: string;
  sizeBytes: number | null;
}

export type ConstraintKind = "primary_key" | "foreign_key" | "unique" | "check" | "exclusion";

export interface ConstraintInfo {
  name: string;
  kind: ConstraintKind;
  columns: string[];
  definition: string;
  references: { schema: string; table: string; columns: string[] } | null;
}

export interface RelationDetail {
  summary: RelationSummary;
  owner: string;
  columns: ColumnInfo[];
  indexes: IndexInfo[];
  constraints: ConstraintInfo[];
  /** CREATE statement reconstructed from the catalog, for reading and copying. */
  ddl: string;
}

/** schema -> relation -> column names. Feeds SQL autocomplete. */
export type CompletionCatalog = Record<string, Record<string, string[]>>;

/* ------------------------------------------------------------------------------------------
 * Query results
 * ---------------------------------------------------------------------------------------- */

/**
 * A typed cell. Exact numerics (numeric, bigint, money) travel as strings so no precision is
 * lost in JavaScript; the UI formats them but never converts them to floating point.
 */
export type Cell =
  | { t: "null" }
  | { t: "bool"; v: boolean }
  | { t: "int"; v: string }
  | { t: "float"; v: number }
  | { t: "decimal"; v: string }
  | { t: "text"; v: string; truncated?: boolean }
  | { t: "date"; v: string }
  | { t: "time"; v: string }
  | { t: "timestamp"; v: string }
  | { t: "timestamptz"; v: string }
  | { t: "uuid"; v: string }
  | { t: "json"; v: string; truncated?: boolean }
  | { t: "bytes"; size: number }
  | { t: "unsupported"; pgType: string };

export interface ResultColumn {
  name: string;
  /** PostgreSQL type name, e.g. "int8", "numeric", "timestamptz". */
  pgType: string;
}

export interface QueryResultSet {
  columns: ResultColumn[];
  rows: Cell[][];
  /** True when more rows exist than the row limit allowed us to fetch. */
  truncated: boolean;
  rowLimit: number;
}

export type QueryErrorCategory = ErrorCategory;

export type QueryOutcome =
  | { ok: true; result: QueryResultSet; durationMs: number; simulated: boolean }
  | {
      ok: false;
      category: QueryErrorCategory;
      message: string;
      hint: string | null;
      /** 1-based character offset into the SQL, when PostgreSQL reports one. */
      position: number | null;
      durationMs: number;
      simulated: boolean;
    };

/* ------------------------------------------------------------------------------------------
 * Report templates
 * ---------------------------------------------------------------------------------------- */

/**
 * A saved report template. `definition` is the versioned template JSON; the templates feature
 * owns its shape and normalises it on load (see src/features/templates/model.ts).
 */
export interface StoredTemplate {
  id: string;
  name: string;
  definition: unknown;
  createdAt: string;
  updatedAt: string;
}

/* ------------------------------------------------------------------------------------------
 * SQL Editor: variables, saved queries and the language service
 * ---------------------------------------------------------------------------------------- */

export const VARIABLE_TYPES = ["text", "number", "date", "date_range", "enum", "multi_enum"] as const;
export type VariableType = (typeof VARIABLE_TYPES)[number];

export interface VariableOption {
  value: string;
  /** Shown instead of the value when not empty. */
  label: string;
}

export type VariableOptionsSource =
  | { source: "list"; values: VariableOption[] }
  /** Read-only lookup query: column 1 is the value, optional column 2 the label. */
  | { source: "query"; sql: string };

export interface DateRangeValue {
  from: string;
  to: string;
}

/**
 * A filter value. Text, number (as a decimal string), date (YYYY-MM-DD) and enum take a string;
 * multi-select enum takes a string array; a date range takes `{ from, to }`. `null`, "" and []
 * mean "no value", which binds NULL for optional variables.
 */
export type VariableValue = string | string[] | DateRangeValue | null;

/** A `{{name}}` variable. Mirrors `VariableDef` in src-tauri/src/lang/variables.rs. */
export interface VariableDef {
  name: string;
  label: string;
  type: VariableType;
  required: boolean;
  default: VariableValue;
  /** Only for enum and multi-select enum. */
  options: VariableOptionsSource | null;
  help: string;
}

export interface SavedQuery {
  id: string;
  name: string;
  description: string;
  sql: string;
  variables: VariableDef[];
  createdAt: string;
  updatedAt: string;
}

export interface SavedQueryDraft {
  name: string;
  description: string;
  sql: string;
  variables: VariableDef[];
}

/** Offsets are UTF-16 code units, exactly as the editor counts them. */
export interface SqlDiagnostic {
  from: number;
  to: number;
  severity: "error" | "warning";
  message: string;
  hint: string | null;
}

export interface SqlVariableReference {
  name: string;
  /** "from" / "to" for date ranges. */
  part: string | null;
  from: number;
  to: number;
}

export interface SqlAnalysis {
  diagnostics: SqlDiagnostic[];
  references: SqlVariableReference[];
  statements: number;
}

export type SqlCompletionKind =
  "keyword" | "function" | "table" | "view" | "column" | "schema" | "type" | "variable" | "alias";

export interface SqlCompletionItem {
  label: string;
  kind: SqlCompletionKind;
  detail: string | null;
  info: string | null;
  /** Text to insert instead of the label. */
  apply: string | null;
  /** `apply` is a snippet where `${}` marks the cursor position. */
  snippet: boolean;
  boost: number;
}

export interface SqlCompletions {
  from: number;
  items: SqlCompletionItem[];
}

export interface SqlSignatureHelp {
  name: string;
  signatures: string[];
  parameters: string[];
  activeParameter: number;
  description: string;
}

export interface SqlParameterInfo {
  /** `region`, or `period.from` for half of a date range. */
  reference: string;
  /** The PostgreSQL type the value is converted to. */
  pgType: string;
}

/** Result of checking a query on the server without running it. */
export type SqlCheckOutcome =
  | { ok: true; columns: ResultColumn[]; parameters: SqlParameterInfo[]; durationMs: number }
  | Extract<QueryOutcome, { ok: false }>;

export type VariableOptionsOutcome =
  /** `column` is set for Multi-select values read from a column: `schema.table.column`. */
  | { ok: true; options: VariableOption[]; truncated: boolean; column?: string | null }
  | Extract<QueryOutcome, { ok: false }>;

/* ------------------------------------------------------------------------------------------
 * Reports
 * ---------------------------------------------------------------------------------------- */

/** Limits enforced by the backend (src-tauri/src/report/mod.rs). */
export const REPORT_PDF_MAX_ROWS = 25_000;
export const REPORT_EXCEL_MAX_ROWS = 1_000_000;

export interface ReportRequest {
  title: string;
  sql: string;
  variables: VariableDef[];
  values: Record<string, VariableValue>;
  /** A normalised template definition. */
  template: unknown;
  pdf: boolean;
  xlsx: boolean;
}

export interface GeneratedReportFile {
  format: "pdf" | "xlsx";
  path: string;
  name: string;
  sizeBytes: number;
  pages: number | null;
}

export type ReportOutcome =
  | {
      ok: true;
      files: GeneratedReportFile[];
      rows: number;
      /** E.g. a format that couldn't be created. */
      warnings: string[];
      durationMs: number;
    }
  | Extract<QueryOutcome, { ok: false }>;

/** Rows a preview reads (src-tauri/src/report/mod.rs). */
export const REPORT_PREVIEW_ROWS = 200;

export type ReportPreviewOutcome =
  | {
      ok: true;
      /** PNG data URLs of the first pages. */
      pages: string[];
      /** Pages laid out from the rows read. */
      pageCount: number;
      rows: number;
      /** The query has more rows than the preview read. */
      moreRows: boolean;
      /** Some long text was shortened to fit the PDF columns. */
      shortened: boolean;
      /** Too many columns for the page width. */
      crowded: boolean;
      durationMs: number;
    }
  | Extract<QueryOutcome, { ok: false }>;

/* ------------------------------------------------------------------------------------------
 * Client
 * ---------------------------------------------------------------------------------------- */

export interface IntalixClient {
  /** True while the UI runs against the simulated backend. */
  readonly simulated: boolean;
  connections: {
    list(): Promise<ConnectionProfile[]>;
    test(draft: ConnectionDraft, signal?: AbortSignal, existingId?: string): Promise<ConnectionTestResult>;
    /** Tests first and saves only if the test succeeds. `id` updates an existing profile. */
    save(draft: ConnectionDraft, signal?: AbortSignal, id?: string): Promise<SaveConnectionResult>;
    /** Deletes the profile and its stored password. */
    remove(id: string): Promise<void>;
    /** Closes pooled connections for a profile. */
    disconnect(id: string): Promise<void>;
  };
  schema: {
    database(connectionId: string): Promise<DatabaseInfo>;
    listSchemas(connectionId: string): Promise<SchemaSummary[]>;
    listObjects(connectionId: string, schema: string): Promise<SchemaObjects>;
    getRelation(connectionId: string, schema: string, name: string): Promise<RelationDetail>;
    completionCatalog(connectionId: string): Promise<CompletionCatalog>;
  };
  query: {
    /** Runs one read-only statement. Rejected statements resolve with `query_rejected`. */
    execute(
      connectionId: string,
      sql: string,
      options: { rowLimit: number },
      signal?: AbortSignal,
    ): Promise<QueryOutcome>;
    /** First rows of a table or view, for the Data tab. */
    preview(
      connectionId: string,
      schema: string,
      name: string,
      options: { rowLimit: number },
      signal?: AbortSignal,
    ): Promise<QueryOutcome>;
  };
  templates: {
    /** Templates the user has saved, most recently changed first. */
    list(): Promise<StoredTemplate[]>;
    /** Creates a template, or replaces the one with `id`. */
    save(name: string, definition: unknown, id?: string): Promise<StoredTemplate>;
    remove(id: string): Promise<void>;
  };
  queries: {
    /** Saved SQL Editor queries, most recently changed first. */
    list(): Promise<SavedQuery[]>;
    /** Creates a saved query, or replaces the one with `id`. */
    save(draft: SavedQueryDraft, id?: string): Promise<SavedQuery>;
    remove(id: string): Promise<void>;
  };
  reports: {
    /**
     * Asks where to save (closing the Save dialog rejects with an AbortError), runs the query in
     * full and writes PDF and/or Excel.
     */
    generate(connectionId: string, request: ReportRequest, signal?: AbortSignal): Promise<ReportOutcome>;
    /** Runs the first rows and draws the first PDF pages. Formats in the request are ignored. */
    preview(connectionId: string, request: ReportRequest, signal?: AbortSignal): Promise<ReportPreviewOutcome>;
    /** Opens a report created in this session, or shows it in File Explorer. */
    open(path: string, reveal: boolean): Promise<void>;
  };
  /** PostgreSQL language service. `variables: null` where variables are not available. */
  sql: {
    analyze(sql: string, variables: VariableDef[] | null): Promise<SqlAnalysis>;
    complete(
      connectionId: string,
      sql: string,
      cursor: number,
      variables: VariableDef[] | null,
    ): Promise<SqlCompletions>;
    signature(connectionId: string, sql: string, cursor: number): Promise<SqlSignatureHelp | null>;
    /** Reloads table and column names used for hints. */
    refreshCatalog(connectionId: string): Promise<void>;
    /** Parses and analyses the query on the server without running it. */
    check(connectionId: string, sql: string, variables: VariableDef[], signal?: AbortSignal): Promise<SqlCheckOutcome>;
    /** Runs a query with `{{variables}}` bound as parameters. Error positions refer to `sql`. */
    run(
      connectionId: string,
      sql: string,
      variables: VariableDef[],
      values: Record<string, VariableValue>,
      options: { rowLimit: number },
      signal?: AbortSignal,
    ): Promise<QueryOutcome>;
    /** Runs an enum variable's lookup query. */
    variableOptions(connectionId: string, sql: string, signal?: AbortSignal): Promise<VariableOptionsOutcome>;
    /** Distinct values of the column `{{variable}}` is compared with in `sql`. */
    columnValues(
      connectionId: string,
      sql: string,
      variable: string,
      signal?: AbortSignal,
    ): Promise<VariableOptionsOutcome>;
  };
}
