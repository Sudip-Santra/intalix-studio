/**
 * DEVELOPMENT-ONLY simulated backend. Nothing here touches a real database.
 *
 * Every result is flagged `simulated: true` so the UI can label it. Profiles live in memory
 * and disappear on reload; passwords are never kept, only a `hasStoredPassword` flag.
 *
 * To exercise the failure states while building the UI:
 *   host containing "fail"      -> connection_failed
 *   host containing "slow"      -> timeout (after the connect timeout, capped at 4s)
 *   password "wrong"            -> authentication_failed
 *   database "missing"          -> database_not_found
 *   sslMode verify-* with host containing "selfsigned" -> tls_error
 *
 * Every connection browses the same sample database from ./mock/catalog.ts.
 */
import type {
  ConnectionDraft,
  ConnectionProfile,
  ConnectionTestResult,
  IntalixClient,
  SaveConnectionResult,
} from "./types";
import {
  findRelation,
  generateRows,
  mockCompletionCatalog,
  mockObjects,
  mockSchemas,
  relationColumns,
  relationDetail,
} from "./mock/catalog";
import { simulateQuery, wait } from "./mock/query";

const profiles: ConnectionProfile[] = [];

function fail(
  category: Extract<ConnectionTestResult, { ok: false }>["category"],
  message: string,
  hint: string | null,
): ConnectionTestResult {
  return { ok: false, category, message, hint, simulated: true };
}

async function simulateTest(
  d: ConnectionDraft,
  signal?: AbortSignal,
  existingId?: string,
): Promise<ConnectionTestResult> {
  const host = d.host.toLowerCase();

  if (host.includes("slow")) {
    await wait(Math.min(d.connectTimeoutSecs * 1000, 4000), signal);
    return fail(
      "timeout",
      `No response from ${d.host}:${d.port} within ${d.connectTimeoutSecs}s.`,
      "The server may be overloaded or blocked by a firewall. Try a longer connect timeout under Advanced.",
    );
  }

  await wait(700 + Math.round(Math.random() * 500), signal);

  if (host.includes("fail")) {
    return fail(
      "connection_failed",
      `Could not reach ${d.host}:${d.port}.`,
      "Check the host name and port, and that you are on the right network or VPN.",
    );
  }
  if (d.sslMode.startsWith("verify") && host.includes("selfsigned")) {
    return fail(
      "tls_error",
      "The server certificate could not be verified.",
      "Provide the CA certificate that signed the server certificate, or ask your DBA for it.",
    );
  }
  const hasPassword =
    d.password.length > 0 ||
    (existingId !== undefined && profiles.some((p) => p.id === existingId && p.hasStoredPassword));
  if (d.password === "wrong" || !hasPassword) {
    return fail(
      "authentication_failed",
      `Password authentication failed for user "${d.username}".`,
      hasPassword ? "Check the username and password." : "Enter the password for this user.",
    );
  }
  if (d.database === "missing") {
    return fail(
      "database_not_found",
      `Database "${d.database}" does not exist on this server.`,
      "Check the database name. Names are case-sensitive when created with quotes.",
    );
  }

  return {
    ok: true,
    serverVersion: "PostgreSQL 16.4",
    latencyMs: 12 + Math.round(Math.random() * 40),
    tls: d.sslMode === "disable" ? null : "TLS 1.3",
    simulated: true,
  };
}

export const mockClient: IntalixClient = {
  simulated: true,
  connections: {
    async list() {
      await wait(150);
      return profiles.map((p) => ({ ...p }));
    },

    test: simulateTest,

    async save(d, signal, id): Promise<SaveConnectionResult> {
      const test = await simulateTest(d, signal, id);
      if (!test.ok) return { test, profile: null };

      const now = new Date().toISOString();
      const existing = id ? profiles.find((p) => p.id === id) : undefined;
      const { password, savePassword, ...rest } = d;
      const profile: ConnectionProfile = {
        ...rest,
        id: existing?.id ?? crypto.randomUUID(),
        hasStoredPassword: savePassword && (password.length > 0 || !!existing?.hasStoredPassword),
        createdAt: existing?.createdAt ?? now,
        lastConnectedAt: now,
      };
      if (existing) profiles.splice(profiles.indexOf(existing), 1, profile);
      else profiles.unshift(profile);
      return { test, profile: { ...profile } };
    },

    async remove(id) {
      await wait(120);
      const index = profiles.findIndex((p) => p.id === id);
      if (index >= 0) profiles.splice(index, 1);
    },

    async disconnect() {
      await wait(50);
    },
  },

  schema: {
    async database(connectionId) {
      await wait(120);
      const profile = profiles.find((p) => p.id === connectionId);
      return {
        name: profile?.database ?? "sample",
        serverVersion: "PostgreSQL 16.4",
        sizeBytes: 8_912_896_000,
        currentUser: profile?.username ?? "report_reader",
      };
    },
    async listSchemas() {
      await wait(180);
      return mockSchemas();
    },
    async listObjects(_connectionId, schema) {
      await wait(220);
      return mockObjects(schema);
    },
    async getRelation(_connectionId, schema, name) {
      await wait(200);
      const rel = findRelation(schema, name);
      if (!rel) throw new Error(`relation "${schema}.${name}" does not exist`);
      return relationDetail(rel);
    },
    async completionCatalog() {
      await wait(150);
      return mockCompletionCatalog();
    },
  },

  query: {
    execute: (_connectionId, sql, { rowLimit }, signal) => simulateQuery(sql, rowLimit, signal),

    async preview(_connectionId, schema, name, { rowLimit }, signal) {
      const started = performance.now();
      await wait(180 + Math.random() * 200, signal);
      const rel = findRelation(schema, name);
      if (!rel) {
        return {
          ok: false,
          category: "undefined_object",
          message: `relation "${schema}.${name}" does not exist`,
          hint: "It may have been dropped. Refresh the explorer.",
          position: null,
          durationMs: Math.round(performance.now() - started),
          simulated: true,
        };
      }
      const count = Math.min(rowLimit, rel.estimatedRows);
      return {
        ok: true,
        durationMs: Math.round(performance.now() - started),
        simulated: true,
        result: {
          columns: relationColumns(rel),
          rows: generateRows(rel, count),
          truncated: rel.estimatedRows > rowLimit,
          rowLimit,
        },
      };
    },
  },
};
