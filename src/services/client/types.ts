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
}
