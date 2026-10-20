import { z } from "zod";
import { ENVIRONMENTS, SSL_MODES, type ConnectionDraft, type ConnectionProfile, type SslMode } from "@/services/client";

// PostgreSQL truncates identifiers longer than 63 bytes (NAMEDATALEN - 1).
const PG_IDENTIFIER_MAX = 63;

const wholeNumber = (label: string, min: number, max: number) =>
  z
    .number({ error: `Enter ${label}` })
    .int(`${label[0]?.toUpperCase()}${label.slice(1)} must be a whole number`)
    .min(min, `Must be at least ${min}`)
    .max(max, `Must be at most ${max}`);

export const connectionSchema = z.object({
  name: z.string().trim().min(1, "Give this connection a name").max(80, "Keep the name under 80 characters"),
  environment: z.enum(ENVIRONMENTS),
  host: z
    .string()
    .trim()
    .min(1, "Enter the server host name or IP address")
    .max(253, "Host names are limited to 253 characters")
    .regex(/^[A-Za-z0-9._\-:[\]]+$/, "Use only letters, digits, dots, hyphens, or an IPv6 address"),
  port: wholeNumber("a port", 1, 65535),
  database: z
    .string()
    .trim()
    .min(1, "Enter the database name")
    .max(PG_IDENTIFIER_MAX, `PostgreSQL names are limited to ${PG_IDENTIFIER_MAX} characters`),
  username: z
    .string()
    .trim()
    .min(1, "Enter the database user")
    .max(PG_IDENTIFIER_MAX, `PostgreSQL names are limited to ${PG_IDENTIFIER_MAX} characters`),
  password: z.string().max(1024, "Password is too long"),
  savePassword: z.boolean(),
  sslMode: z.enum(SSL_MODES),
  caCertPath: z.string().trim().max(1024, "Path is too long"),
  connectTimeoutSecs: wholeNumber("a timeout", 1, 120),
  queryTimeoutSecs: wholeNumber("a timeout", 1, 3600),
  maxConnections: wholeNumber("a limit", 1, 20),
  applicationName: z.string().trim().max(PG_IDENTIFIER_MAX, `Limited to ${PG_IDENTIFIER_MAX} characters`),
}) satisfies z.ZodType<ConnectionDraft>;

export type ConnectionFormValues = z.infer<typeof connectionSchema>;

export const DEFAULT_VALUES: ConnectionFormValues = {
  name: "",
  environment: "development",
  host: "",
  port: 5432,
  database: "",
  username: "",
  password: "",
  savePassword: true,
  sslMode: "verify-full",
  caCertPath: "",
  connectTimeoutSecs: 10,
  queryTimeoutSecs: 120,
  maxConnections: 4,
  applicationName: "Intalix Studio",
};

/** Fields whose change makes a previous connection test result stale. */
export const CONNECTION_FIELDS = new Set<keyof ConnectionFormValues>([
  "host",
  "port",
  "database",
  "username",
  "password",
  "sslMode",
  "caCertPath",
  "connectTimeoutSecs",
]);

export function profileToValues(p: ConnectionProfile): ConnectionFormValues {
  return {
    name: p.name,
    environment: p.environment,
    host: p.host,
    port: p.port,
    database: p.database,
    username: p.username,
    password: "",
    savePassword: p.hasStoredPassword,
    sslMode: p.sslMode,
    caCertPath: p.caCertPath,
    connectTimeoutSecs: p.connectTimeoutSecs,
    queryTimeoutSecs: p.queryTimeoutSecs,
    maxConnections: p.maxConnections,
    applicationName: p.applicationName,
  };
}

const LOCAL_HOSTS = new Set(["localhost", "127.0.0.1", "::1", "[::1]"]);
export const isLocalHost = (host: string) => LOCAL_HOSTS.has(host.trim().toLowerCase());

export type ParsedConnectionString =
  { ok: true; values: Partial<ConnectionFormValues>; notes: string[] } | { ok: false; error: string };

/**
 * Parses a libpq-style URL into form values. The password, if present, goes into the form
 * only; the string itself is never stored.
 *
 * @example
 * parseConnectionString("postgresql://ana@db.acme.io:6432/sales?sslmode=require")
 * // => { ok: true, values: { host: "db.acme.io", port: 6432, database: "sales",
 * //                          username: "ana", sslMode: "require" }, notes: [] }
 * parseConnectionString("mysql://x") // => { ok: false, error: "Connection strings must start with ..." }
 */
export function parseConnectionString(raw: string): ParsedConnectionString {
  const input = raw.trim();
  const scheme = /^postgres(?:ql)?:\/\//i.exec(input);
  if (!scheme) {
    return { ok: false, error: "Connection strings must start with postgresql:// or postgres://" };
  }
  const rest = input.slice(scheme[0].length);
  const authority = rest.split(/[/?]/, 1)[0] ?? "";
  if (authority.slice(authority.lastIndexOf("@") + 1).includes(",")) {
    return { ok: false, error: "Connection strings with multiple hosts are not supported yet." };
  }

  let url: URL;
  try {
    url = new URL(`http://${rest}`);
  } catch {
    return { ok: false, error: "This doesn't look like a valid connection string." };
  }

  const decode = (s: string) => {
    try {
      return decodeURIComponent(s);
    } catch {
      return s;
    }
  };

  const values: Partial<ConnectionFormValues> = {};
  const notes: string[] = [];

  if (url.hostname) values.host = url.hostname;
  values.port = url.port ? Number(url.port) : 5432;
  const database = decode(url.pathname.replace(/^\//, ""));
  if (database) values.database = database;
  if (url.username) values.username = decode(url.username);
  if (url.password) {
    values.password = decode(url.password);
    notes.push("Password copied into the form. It is not kept as part of the connection string.");
  }

  const sslmode = url.searchParams.get("sslmode");
  if (sslmode) {
    if ((SSL_MODES as readonly string[]).includes(sslmode)) values.sslMode = sslmode as SslMode;
    else if (sslmode === "allow") {
      values.sslMode = "prefer";
      notes.push('sslmode "allow" is not offered; using "prefer" instead.');
    } else notes.push(`Unknown sslmode "${sslmode}" was ignored.`);
  }
  const timeout = Number(url.searchParams.get("connect_timeout"));
  if (Number.isInteger(timeout) && timeout >= 1 && timeout <= 120) values.connectTimeoutSecs = timeout;
  const appName = url.searchParams.get("application_name");
  if (appName) values.applicationName = appName.slice(0, PG_IDENTIFIER_MAX);

  return { ok: true, values, notes };
}

/** Password-free summary for display, e.g. `ana@db.acme.io:5432/sales`. */
export function describeTarget(p: Pick<ConnectionProfile, "username" | "host" | "port" | "database">) {
  return `${p.username}@${p.host}:${p.port}/${p.database}`;
}
